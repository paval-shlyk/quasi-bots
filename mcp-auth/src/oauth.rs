/// OAuth 2.1 Authorization Code + PKCE flow.
///
/// The MCP server acts as its own Authorization Server:
///   GET  /.well-known/oauth-authorization-server  — RFC8414 metadata
///   POST /oauth/register                          — RFC7591 dynamic client reg
///   GET  /oauth/authorize                         — consent + Google sign-in
///   GET  /oauth/google/login                      — redirect to Google OIDC
///   GET  /oauth/google/callback                   — Google OIDC callback
///   POST /oauth/token                             — code/refresh → Bearer token
///
/// Owner authentication uses Google OIDC; only allowlisted `sub` values may approve access.
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    routing::{get, post},
};
use tracing::warn;

use crate::config::McpAuthConfig;

mod google;
mod metadata;
mod routes;
mod store;
mod token;

pub struct OAuthState {
    pub store: store::OAuthStore,
    pub config: McpAuthConfig,
    pub google: Option<google::GoogleAuth>,
}

pub type SharedOAuthState = Arc<OAuthState>;

pub async fn state(
    config: McpAuthConfig,
) -> anyhow::Result<SharedOAuthState> {
    let store = store::OAuthStore::new();

    let google = if config.auth.google_configured() {
        let redirect_uri = config.auth.google_redirect_uri(&config.public_url);
        match google::build_google_auth(
            &config.auth.google.client_id,
            &config.auth.resolve_client_secret().expect("checked"),
            &redirect_uri,
        )
        .await
        {
            Ok(g) => Some(g),
            Err(e) => {
                warn!(error = %e, "failed to initialize Google OIDC client");
                None
            }
        }
    } else {
        warn!(
            "Google OAuth not fully configured; set auth.google.client_id and GOOGLE_CLIENT_SECRET"
        );
        None
    };

    if config.auth.dev_allowlist_mode() {
        warn!(
            "auth allowlist is empty — any Google account can authorize (dev mode); add auth.google.allowed_google_subs for production"
        );
    }

    Ok(Arc::new(OAuthState {
        store,
        config,
        google,
    }))
}

pub fn router() -> Router<SharedOAuthState> {
    use tower_http::cors::{Any, CorsLayer};

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(routes::protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(routes::protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(routes::metadata),
        )
        .route("/oauth/register", post(routes::register))
        .route("/oauth/authorize", get(routes::authorize))
        .route("/oauth/google/login", get(google::login))
        .route("/oauth/google/callback", get(google::callback))
        .route("/oauth/token", post(routes::authorize_or_refresh_token))
        .layer(cors)
}

pub async fn bearer_auth_middleware(
    State(state): State<SharedOAuthState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let store = &state.store;

    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    match token {
        Some(t) if store.validate_token(&t).await => next.run(request).await,
        _ => {
            let challenge = metadata::www_authenticate_challenge(&state.config);
            let body = serde_json::json!({
                "error": "invalid_token",
                "error_description": "valid Bearer token required"
            });
            let mut resp = Response::new(Body::from(body.to_string()));
            *resp.status_mut() = StatusCode::UNAUTHORIZED;
            resp.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_str(&challenge).unwrap_or_else(
                    |_| axum::http::HeaderValue::from_static("Bearer"),
                ),
            );
            resp.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            );
            resp
        }
    }
}

