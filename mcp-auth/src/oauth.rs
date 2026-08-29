//! MCP resource-server OAuth helpers.
//!
//! The host is an OAuth 2.1 **resource server**. Authorization is delegated to
//! an external authorization server (Keycloak, Zitadel, …):
//!
//!   GET  /.well-known/oauth-protected-resource       — RFC 9728 metadata
//!   GET  /.well-known/oauth-protected-resource/mcp   — path-scoped PRM
//!
//! Bearer tokens on `/mcp` are JWTs validated via the AS JWKS (OIDC discovery).

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    routing::get,
};
use tracing::debug;

use crate::{config::McpAuthConfig, oauth::jwt::new_cached_jwks};

mod discovery;
mod jwt;
mod metadata;
mod routes;

pub struct OAuthState {
    pub config: McpAuthConfig,
    pub jwks: jwt::CachedJwkSet,
}

pub type SharedOAuthState = Arc<OAuthState>;

pub use jwt::{JwtError, JwtValidator, ValidatedToken};

pub async fn state(config: McpAuthConfig) -> anyhow::Result<SharedOAuthState> {
    let jwks = new_cached_jwks(config.clone()).await?;

    Ok(Arc::new(OAuthState { config, jwks }))
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
        .layer(cors)
}

pub async fn bearer_auth_middleware(
    State(state): State<SharedOAuthState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // `resource_url()` allocates; keep it alive for the validator borrows.
    let resource = state.config.resource_url();
    let validator = JwtValidator {
        jwks: state.jwks.clone(),
        expected_audience: &resource,
        expected_issuer: &state.config.authorization_server,
        expected_scope: &state.config.scope,
        allowed_subs: &state.config.allowed_subs,
    };

    match token {
        Some(t) => match validator.validate(&t).await {
            Ok(validated) => {
                debug!(sub = %validated.sub, "MCP bearer JWT accepted");
                next.run(request).await
            }
            Err(JwtError::InsufficientScope) => {
                unauthorized_response(&state.config, "insufficient_scope", true)
            }
            Err(JwtError::SubjectNotAllowed) => {
                unauthorized_response(&state.config, "invalid_token", false)
            }
            Err(e) => {
                debug!(error = %e, "MCP bearer JWT rejected");
                unauthorized_response(&state.config, "invalid_token", false)
            }
        },
        None => unauthorized_response(&state.config, "invalid_token", false),
    }
}

fn unauthorized_response(
    config: &McpAuthConfig,
    error: &str,
    insufficient_scope: bool,
) -> Response {
    let challenge = if insufficient_scope {
        format!(
            r#"Bearer realm="mcp", error="insufficient_scope", scope="{}", resource_metadata="{}""#,
            config.scope,
            config.protected_resource_metadata_url(),
        )
    } else {
        metadata::www_authenticate_challenge(config)
    };

    let status = if insufficient_scope {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::UNAUTHORIZED
    };

    let description = match error {
        "insufficient_scope" => "token is missing required scope",
        _ => "valid Bearer JWT required",
    };
    let body = serde_json::json!({
        "error": error,
        "error_description": description,
    });

    let mut resp = Response::new(Body::from(body.to_string()));
    *resp.status_mut() = status;
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
