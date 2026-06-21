/// OAuth 2.1 Authorization Code + PKCE flow.
///
/// The MCP server acts as its own Authorization Server:
///   GET  /.well-known/oauth-authorization-server  — RFC8414 metadata
///   POST /oauth/register                          — RFC7591 dynamic client reg
///   GET  /oauth/authorize                         — login + consent HTML
///   POST /oauth/approve                           — form submit → auth code
///   POST /oauth/token                             — code/refresh → Bearer token
///
/// Only one owner account is supported; credentials come from config.toml.
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
use crate::config::McpServerConfig;

mod metadata;
mod routes;
mod store;
mod token;

pub struct OAuthState {
    pub store: store::OAuthStore,
    pub config: McpServerConfig,
}

pub type SharedOAuthState = Arc<OAuthState>;

pub fn state(config: McpServerConfig) -> SharedOAuthState {
    Arc::new(OAuthState {
        store: store::OAuthStore::new(),
        config,
    })
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
        .route("/oauth/approve", post(routes::approve))
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
                axum::http::HeaderValue::from_str(&challenge)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("Bearer")),
            );
            resp.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            );
            resp
        }
    }
}
