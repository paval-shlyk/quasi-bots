use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    EndpointMaybeSet, EndpointNotSet, EndpointSet, IssuerUrl, Nonce,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
    core::{CoreClient, CoreProviderMetadata, CoreResponseType},
};
use reqwest::redirect::Policy;
use serde::Deserialize;
use tracing::info;

use super::{SharedOAuthState, store::AuthSession};

pub type GoogleOidcClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

pub struct GoogleAuth {
    pub client: GoogleOidcClient,
    pub http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct GoogleLoginQuery {
    pub pending: String,
}

#[derive(Debug, Deserialize)]
pub struct GoogleCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

// GET /oauth/google/login
pub async fn login(
    Query(query): Query<GoogleLoginQuery>,
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    let google = match &state.google {
        Some(g) => g,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Html("<h1>Google OAuth is not configured</h1><p>Set auth.google.client_id and GOOGLE_CLIENT_SECRET.</p>"),
            )
                .into_response();
        }
    };

    if state.store.get_pending_auth(&query.pending).await.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Html("<h1>Authorization session expired</h1><p>Restart the MCP client OAuth flow.</p>"),
        )
            .into_response();
    }

    let (pkce_challenge, pkce_verifier) =
        PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_state, nonce) = google
        .client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".into()))
        .add_scope(Scope::new("email".into()))
        .add_scope(Scope::new("profile".into()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    state
        .store
        .save_google_state(
            csrf_state.secret(),
            &query.pending,
            pkce_verifier.secret(),
            nonce.secret(),
        )
        .await;

    Redirect::to(auth_url.as_str()).into_response()
}

// GET /oauth/google/callback
pub async fn callback(
    Query(query): Query<GoogleCallbackQuery>,
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    if let Some(error) = query.error {
        return (
            StatusCode::BAD_REQUEST,
            Html(format!("<h1>Google sign-in failed</h1><p>{error}</p>")),
        )
            .into_response();
    }

    let google = match &state.google {
        Some(g) => g,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Html("<h1>Google OAuth is not configured</h1>"),
            )
                .into_response();
        }
    };

    let code = match query.code {
        Some(c) if !c.is_empty() => AuthorizationCode::new(c),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Html("<h1>Missing authorization code</h1>"),
            )
                .into_response();
        }
    };

    let csrf_state = match query.state {
        Some(s) if !s.is_empty() => s,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Html("<h1>Missing OAuth state</h1>"),
            )
                .into_response();
        }
    };

    let (pending_id, pkce_verifier, nonce) =
        match state.store.take_google_state(&csrf_state).await {
            Some(v) => v,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Html("<h1>Invalid or expired OAuth state</h1>"),
                )
                    .into_response();
            }
        };

    let pending = match state.store.take_pending_auth(&pending_id).await {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Html("<h1>Authorization session expired</h1>"),
            )
                .into_response();
        }
    };

    let token_response = match google.client.exchange_code(code) {
        Ok(request) => {
            match request
                .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
                .request_async(&google.http)
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Html(format!("<h1>Google token exchange failed</h1><pre>{e}</pre>")),
                    )
                        .into_response();
                }
            }
        }
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Html(format!("<h1>Google token endpoint not configured</h1><pre>{e}</pre>")),
            )
                .into_response();
        }
    };

    let id_token = match token_response.id_token() {
        Some(t) => t,
        None => {
            return (
                StatusCode::BAD_GATEWAY,
                Html("<h1>Google did not return an id_token</h1>"),
            )
                .into_response();
        }
    };

    let id_token_verifier = google.client.id_token_verifier();
    let nonce = Nonce::new(nonce);
    let claims = match id_token.claims(&id_token_verifier, &nonce) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Html(format!("<h1>Invalid Google id_token</h1><pre>{e}</pre>")),
            )
                .into_response();
        }
    };

    let sub = claims.subject().to_string();
    let email = claims
        .email()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "(no email claim)".into());

    if !state.config.auth.owner_allowed(&sub) {
        return (
            StatusCode::FORBIDDEN,
            Html(format!(
                r#"<!DOCTYPE html><html><body style="font-family:sans-serif;padding:2rem">
                <h1>Access denied</h1>
                <p>Google account <strong>{email}</strong> is not on the owner allowlist.</p>
                <p>Google <code>sub</code>: <code>{sub}</code></p>
                </body></html>"#
            )),
        )
            .into_response();
    }

    if state.config.auth.dev_allowlist_mode() {
        info!(
            google_sub = %sub,
            email = %email,
            "dev allowlist mode: add this sub to auth.google.allowed_google_subs"
        );
    }

    let dev_notice = if state.config.auth.dev_allowlist_mode() {
        format!(
            r#"<div style="background:#1a2e1a;border:1px solid #2d5a2d;color:#9ae6b4;padding:0.75rem;border-radius:6px;margin-bottom:1rem;font-size:0.85rem">
            Dev mode: add <code>{sub}</code> to <code>auth.google.allowed_google_subs</code> in config.
            </div>"#
        )
    } else {
        String::new()
    };

    let auth_code = state
        .store
        .save_session(AuthSession {
            client_id: pending.client_id.clone(),
            redirect_uri: pending.redirect_uri.clone(),
            scope: pending.scope.clone(),
            state: pending.state.clone(),
            code_challenge: pending.code_challenge.clone(),
            resource: pending.resource.clone(),
            owner_sub: Some(sub.clone()),
        })
        .await;

    let mut url = format!("{}?code={}", pending.redirect_uri, auth_code);
    if let Some(s) = pending.state.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&state={s}"));
    }

    if state.config.auth.dev_allowlist_mode() {
        return Html(format!(
            r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Authorised</title></head>
            <body style="font-family:sans-serif;background:#0f1117;color:#e2e8f0;padding:2rem">
            {dev_notice}
            <h1>Signed in as {email}</h1>
            <p>Google <code>sub</code>: <code>{sub}</code></p>
            <p><a href="{url}" style="color:#63b3ed">Continue to MCP client</a></p>
            <script>setTimeout(() => window.location.href = "{url}", 1500);</script>
            </body></html>"#
        ))
        .into_response();
    }

    Redirect::to(&url).into_response()
}

pub async fn build_google_auth(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> anyhow::Result<GoogleAuth> {
    let http = reqwest::Client::builder()
        .redirect(Policy::none())
        .build()?;

    let issuer = IssuerUrl::new("https://accounts.google.com".into())?;
    let metadata = CoreProviderMetadata::discover_async(issuer, &http).await?;

    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(client_id.into()),
        Some(ClientSecret::new(client_secret.into())),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_uri.into())?);

    Ok(GoogleAuth { client, http })
}
