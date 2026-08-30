use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use oauth2::TokenResponse;
use rmcp::transport::auth::{
    AuthorizationManager, AuthorizationMetadata, OAuthClientConfig,
    OAuthTokenResponse,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;

use crate::config::ConnectOptions;
use crate::{Error, Result};

/// Run OAuth 2.1 + PKCE for an MCP resource server (Zitadel-oriented).
///
/// - DCR with `application_type: "native"` + `token_endpoint_auth_method: "none"`
/// - Authorization code + PKCE
/// - Returns the opaque (or JWT) **access_token** for `Authorization: Bearer`
pub async fn login_oauth(opts: &ConnectOptions) -> Result<String> {
    tracing::info!(
        mcp_url = %opts.url,
        redirect = %opts.oauth_redirect,
        scope = %opts.scope,
        client_name = %opts.client_name,
        "starting MCP OAuth login"
    );

    let redirect = Url::parse(&opts.oauth_redirect)
        .map_err(|e| Error::InvalidUrl(format!("redirect URI: {e}")))?;

    let host = redirect.host_str().ok_or_else(|| {
        Error::InvalidUrl("redirect URI must have a host".into())
    })?;
    let port = redirect.port_or_known_default().ok_or_else(|| {
        Error::InvalidUrl("redirect URI must have a port".into())
    })?;
    let path = if redirect.path().is_empty() {
        "/"
    } else {
        redirect.path()
    };

    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
        Error::InvalidUrl(format!("redirect bind address: {e}"))
    })?;

    let listener = TcpListener::bind(addr).await.map_err(|e| {
        Error::Oauth(format!(
            "failed to bind OAuth redirect listener on {addr}: {e}"
        ))
    })?;
    tracing::info!(%addr, %path, "OAuth redirect listener ready");

    let (tx, rx) = oneshot::channel::<Result<(String, String)>>();
    let expected_path = path.to_string();
    tokio::spawn(async move {
        let result = accept_callback(&listener, &expected_path).await;
        let _ = tx.send(result);
    });

    let mut manager = AuthorizationManager::new(opts.url.as_str())
        .await
        .map_err(Error::oauth)?;
    let metadata = manager.discover_metadata().await.map_err(|e| {
        tracing::error!(error = %e, "OAuth discovery / AS metadata failed");
        Error::oauth(e)
    })?;
    tracing::info!(
        authorization_endpoint = %metadata.authorization_endpoint,
        token_endpoint = %metadata.token_endpoint,
        registration_endpoint = ?metadata.registration_endpoint,
        "authorization server metadata"
    );
    manager.set_metadata(metadata.clone());

    let scopes = resolve_scopes(&manager, opts);
    let scope_refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
    tracing::info!(?scopes, "requesting OAuth scopes");

    let client_id = register_native_public_client(
        &metadata,
        &opts.client_name,
        &opts.oauth_redirect,
        &scopes,
    )
    .await?;
    tracing::info!(%client_id, "DCR succeeded (native public client)");

    manager
        .configure_client(
            OAuthClientConfig::new(
                client_id.clone(),
                opts.oauth_redirect.clone(),
            )
            .with_scopes(scopes.clone()),
        )
        .map_err(Error::oauth)?;

    let auth_url = manager.get_authorization_url(&scope_refs).await.map_err(
        |e| {
            tracing::error!(error = %e, "failed to build authorization URL");
            Error::oauth(e)
        },
    )?;
    let auth_url = with_select_account_prompt(&auth_url)?;
    tracing::info!(%auth_url, "authorization URL ready (prompt=select_account)");
    eprintln!("\n=== MCP OAuth ===");
    eprintln!("Open this URL in a browser to authorize:\n{auth_url}\n");
    open_browser(&auth_url);

    tracing::info!("waiting for OAuth redirect callback…");
    let (code, state) = rx
        .await
        .map_err(|_| Error::Oauth("OAuth callback channel closed".into()))??;

    tracing::info!(
        code_len = code.len(),
        "authorization code received; exchanging at token endpoint"
    );

    let token_response = manager
        .exchange_code_for_token(&code, &state)
        .await
        .map_err(|e| {
        tracing::error!(error = %e, "token endpoint / code exchange failed");
        Error::oauth(e)
    })?;

    let access_token = token_response.access_token().secret().to_string();
    log_token_endpoint_response(&token_response);

    Ok(access_token)
}

/// CLI / default scopes, whitespace-split.
fn cli_scopes(opts: &ConnectOptions) -> Vec<String> {
    opts.scope
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Prefer RFC 9728 PRM `scopes_supported`; `--scope` is only the fallback.
fn resolve_scopes(
    manager: &AuthorizationManager,
    opts: &ConnectOptions,
) -> Vec<String> {
    let defaults = cli_scopes(opts);
    let default_refs: Vec<&str> = defaults.iter().map(String::as_str).collect();
    with_oidc_scopes(manager.select_scopes(None, &default_refs))
}

fn with_oidc_scopes(mut scopes: Vec<String>) -> Vec<String> {
    if !scopes.iter().any(|s| s == "openid") {
        scopes.insert(0, "openid".into());
    }
    if !scopes.is_empty() && !scopes.iter().any(|s| s == "offline_access") {
        scopes.push("offline_access".into());
    }
    scopes
}

async fn register_native_public_client(
    metadata: &AuthorizationMetadata,
    client_name: &str,
    redirect_uri: &str,
    scopes: &[String],
) -> Result<String> {
    let registration_url = metadata.registration_endpoint.as_ref().ok_or_else(|| {
        Error::Oauth(
            "AS metadata has no registration_endpoint — enable Zitadel dynamic client registration \
             (dynamicClientRegistration.enabled + allowUnauthenticated for MCP)"
                .into(),
        )
    })?;

    let body = serde_json::json!({
        "client_name": client_name,
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "application_type": "native",
        "scope": scopes.join(" "),
    });

    tracing::info!(%registration_url, "POST dynamic client registration");

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Oauth(e.to_string()))?;

    let response = http
        .post(registration_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Oauth(format!("DCR request failed: {e}")))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| Error::Oauth(format!("DCR read body failed: {e}")))?;

    if !status.is_success() {
        tracing::error!(%status, body = %text, "DCR rejected");
        return Err(Error::Oauth(format!("DCR HTTP {status}: {text}")));
    }

    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| {
            Error::Oauth(format!("DCR JSON parse failed: {e}; body={text}"))
        })?;

    value
        .get("client_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            Error::Oauth(format!("DCR response missing client_id: {text}"))
        })
}

fn log_token_endpoint_response(token: &OAuthTokenResponse) {
    let access = token.access_token().secret();
    let expires_in = token.expires_in().map(|d| d.as_secs());
    let scopes: Option<Vec<String>> = token
        .scopes()
        .map(|s| s.iter().map(|x| x.to_string()).collect());
    let has_refresh = token.refresh_token().is_some();
    let has_id_token = token.extra_fields().0.contains_key("id_token");

    tracing::info!(
        expires_in_secs = ?expires_in,
        scopes = ?scopes,
        access_token_len = access.len(),
        has_refresh_token = has_refresh,
        has_id_token,
        "token endpoint response"
    );

    eprintln!("\n=== Access token ===");
    eprintln!("len={} preview={}", access.len(), token_preview(access));
    eprintln!("expires_in={expires_in:?} scope={scopes:?}");
}

fn token_preview(token: &str) -> String {
    let token = token.trim();
    if token.len() > 16 {
        format!("{}…{}", &token[..8], &token[token.len() - 6..])
    } else if token.is_empty() {
        "(empty)".into()
    } else {
        token.to_string()
    }
}

async fn accept_callback(
    listener: &TcpListener,
    expected_path: &str,
) -> Result<(String, String)> {
    let (mut socket, peer) = listener.accept().await?;
    tracing::debug!(%peer, "OAuth callback connection");

    let mut buf = vec![0u8; 8192];
    let n = socket.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let first_line = request.lines().next().unwrap_or("");
    tracing::debug!(%first_line, "OAuth callback request line");
    let target = first_line.split_whitespace().nth(1).ok_or_else(|| {
        Error::Oauth(format!("malformed callback request: {first_line}"))
    })?;

    let full = format!("http://localhost{target}");
    let parsed = Url::parse(&full).map_err(|e| {
        Error::Oauth(format!("failed to parse callback target: {e}"))
    })?;

    if parsed.path() != expected_path
        && parsed.path() != expected_path.trim_end_matches('/')
    {
        tracing::warn!(
            got = %parsed.path(),
            expected = %expected_path,
            "callback path mismatch"
        );
    }

    let params: HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();

    if let Some(err) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(|s| s.as_str())
            .unwrap_or("");
        tracing::error!(error = %err, description = %desc, "authorization denied by AS");
        let body = html_page(
            "Authorization failed",
            &format!(
                "<p><b>{err}</b></p><p>{desc}</p><p>You can close this window.</p>"
            ),
        );
        write_http_response(&mut socket, 400, &body).await?;
        return Err(Error::Oauth(format!(
            "authorization denied: {err} {desc}"
        )));
    }

    let code = params
        .get("code")
        .cloned()
        .ok_or_else(|| Error::Oauth("callback missing code".into()))?;
    let state = params
        .get("state")
        .cloned()
        .ok_or_else(|| Error::Oauth("callback missing state".into()))?;

    let body = html_page(
        "Authorization complete",
        "<p>You can close this window and return to mcp-client.</p>",
    );
    write_http_response(&mut socket, 200, &body).await?;

    Ok((code, state))
}

async fn write_http_response(
    socket: &mut tokio::net::TcpStream,
    status: u16,
    body: &str,
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );
    socket.write_all(response.as_bytes()).await?;
    socket.shutdown().await.ok();
    Ok(())
}

fn html_page(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{title}</title></head>\
         <body style=\"font-family: sans-serif; max-width: 40rem; margin: 2rem auto;\">\
         <h1>{title}</h1>{body}</body></html>"
    )
}

fn open_browser(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Append Zitadel/OIDC `prompt=select_account` so the user can pick a session
/// (or start a new login) instead of being silently bound to an existing one.
fn with_select_account_prompt(auth_url: &str) -> Result<String> {
    let mut url = Url::parse(auth_url).map_err(|e| {
        Error::Oauth(format!("invalid authorization URL: {e}"))
    })?;

    let already_selects = url.query_pairs().any(|(key, value)| {
        key == "prompt"
            && value
                .split_whitespace()
                .any(|part| part == "select_account")
    });
    if !already_selects {
        url.query_pairs_mut()
            .append_pair("prompt", "select_account");
    }

    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::{with_oidc_scopes, with_select_account_prompt};

    #[test]
    fn with_oidc_scopes_keeps_prm_and_adds_openid() {
        let scopes = with_oidc_scopes(vec![
            "mcp".into(),
            "urn:zitadel:iam:org:project:id:my_client_id:aud".into(),
        ]);
        assert_eq!(
            scopes,
            vec![
                "openid".to_string(),
                "mcp".to_string(),
                "urn:zitadel:iam:org:project:id:my_client_id:aud".to_string(),
                "offline_access".to_string(),
            ]
        );
    }

    #[test]
    fn select_account_prompt_is_appended() {
        let url = with_select_account_prompt(
            "https://auth.example.com/oauth/v2/authorize?client_id=abc&response_type=code",
        )
        .unwrap();
        let parsed = url::Url::parse(&url).unwrap();
        let prompts: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert!(
            prompts
                .iter()
                .any(|(k, v)| k == "prompt" && v == "select_account"),
            "prompt=select_account missing from {url}"
        );
    }

    #[test]
    fn select_account_prompt_is_not_duplicated() {
        let url = with_select_account_prompt(
            "https://auth.example.com/oauth/v2/authorize?prompt=select_account",
        )
        .unwrap();
        let count = url::Url::parse(&url)
            .unwrap()
            .query_pairs()
            .filter(|(k, v)| k == "prompt" && v == "select_account")
            .count();
        assert_eq!(count, 1);
    }
}

/// Shared token holder so the TUI can update credentials after OAuth.
#[derive(Debug, Clone, Default)]
pub struct TokenStore {
    inner: Arc<tokio::sync::RwLock<Option<String>>>,
}

impl TokenStore {
    pub fn new(token: Option<String>) -> Self {
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(token)),
        }
    }

    pub async fn get(&self) -> Option<String> {
        self.inner.read().await.clone()
    }

    pub async fn set(&self, token: String) {
        *self.inner.write().await = Some(token);
    }
}
