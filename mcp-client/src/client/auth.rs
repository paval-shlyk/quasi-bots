use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use rmcp::transport::auth::OAuthState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;

use crate::config::ConnectOptions;
use crate::{Error, Result};

/// Run OAuth 2.1 + PKCE against the MCP resource server origin.
///
/// Returns the access token (without the `Bearer ` prefix).
///
/// Flow:
/// 1. Dynamic client registration at the authorization server
/// 2. Local redirect listener on `opts.oauth_redirect`
/// 3. Browser authorization (Google owner login on skill-master)
/// 4. Code exchange → access token
pub async fn login_oauth(opts: &ConnectOptions) -> Result<String> {
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

    tracing::info!("OAuth redirect listener on http://{addr}{path}");

    let (tx, rx) = oneshot::channel::<Result<(String, String)>>();
    let expected_path = path.to_string();
    tokio::spawn(async move {
        let result = accept_callback(&listener, &expected_path).await;
        let _ = tx.send(result);
    });

    let mut oauth = OAuthState::new(&opts.url, None)
        .await
        .map_err(Error::oauth)?;

    let scopes: Vec<&str> = if opts.scope.is_empty() {
        vec![]
    } else {
        vec![opts.scope.as_str()]
    };

    oauth
        .start_authorization(
            &scopes,
            opts.oauth_redirect.as_str(),
            Some(opts.client_name.as_str()),
        )
        .await
        .map_err(Error::oauth)?;

    let auth_url = oauth.get_authorization_url().await.map_err(Error::oauth)?;
    tracing::info!("Open authorization URL:\n{auth_url}");
    eprintln!("\n=== MCP OAuth ===");
    eprintln!("Open this URL in a browser to authorize:\n{auth_url}\n");
    open_browser(&auth_url);

    let (code, state) = rx
        .await
        .map_err(|_| Error::Oauth("OAuth callback channel closed".into()))??;

    oauth
        .handle_callback(&code, &state)
        .await
        .map_err(Error::oauth)?;

    // rmcp's OAuthState::get_access_token does not work in Authorized state;
    // pull the manager out and read the token from there.
    let manager = oauth.into_authorization_manager().ok_or_else(|| {
        Error::Oauth("OAuth completed but manager missing".into())
    })?;

    let token = manager.get_access_token().await.map_err(Error::oauth)?;
    Ok(token)
}

async fn accept_callback(
    listener: &TcpListener,
    expected_path: &str,
) -> Result<(String, String)> {
    let (mut socket, peer) = listener.accept().await?;
    tracing::debug!("OAuth callback connection from {peer}");

    let mut buf = vec![0u8; 8192];
    let n = socket.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let first_line = request.lines().next().unwrap_or("");
    // GET /callback?code=...&state=... HTTP/1.1
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
        // Still try to extract params; some browsers may normalize path.
        tracing::warn!(
            "callback path mismatch: got {}, expected {expected_path}",
            parsed.path()
        );
    }

    let params: HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();

    if let Some(err) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(|s| s.as_str())
            .unwrap_or("");
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
    // Best-effort; always print the URL so the user can open it manually.
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
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
