//! MCP resource-server OAuth helpers.
//!
//! The host is an OAuth 2.1 **resource server**. Authorization is delegated to
//! an external authorization server (e.g. Zitadel):
//!
//!   GET  /.well-known/oauth-protected-resource       — RFC 9728 metadata
//!   GET  /.well-known/oauth-protected-resource/mcp   — path-scoped PRM
//!
//! Opaque Bearer access tokens on `/mcp` are validated via RFC 7662
//! token introspection against the AS.

use std::sync::Arc;
use std::time::Duration;

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

use crate::config::McpAuthConfig;
use crate::oauth::openid_config::new_cached_openid_configuration;

mod introspect;
mod metadata;
mod openid_config;
mod routes;

pub use introspect::{AuthError, TokenValidator, ValidatedToken};
pub use openid_config::{CachedOpenIdConfig, OpenIdConfiguration};

pub struct OAuthState {
    pub config: McpAuthConfig,
    pub openid_config: CachedOpenIdConfig,
    pub http: reqwest::Client,
    /// Resolved at boot from config / `INTROSPECTION_CLIENT_SECRET`.
    pub introspection_client_secret: String,
}

pub type SharedOAuthState = Arc<OAuthState>;

pub async fn state(config: McpAuthConfig) -> anyhow::Result<SharedOAuthState> {
    let secret = config
        .introspection_client_secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "introspection client secret missing: set introspection_client_secret \
                 in config or INTROSPECTION_CLIENT_SECRET in the environment"
            )
        })?
        .to_string();

    let openid_config = new_cached_openid_configuration(&config).await?;

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    Ok(Arc::new(OAuthState {
        config,
        openid_config,
        http,
        introspection_client_secret: secret,
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

    let resource = state.config.resource_url();
    let validator = TokenValidator {
        openid_config: state.openid_config.clone(),
        client: &state.http,
        client_id: &state.config.introspection_client_id,
        client_secret: &state.introspection_client_secret,
        expected_issuer: &state.config.authorization_server,
        expected_audience: &resource,
        expected_scope: &state.config.scope,
        allowed_subs: &state.config.allowed_subs,
    };

    match token {
        Some(t) => match validator.validate(&t).await {
            Ok(validated) => {
                debug!(
                    sub = %validated.sub,
                    "MCP bearer token accepted (introspection)"
                );
                next.run(request).await
            }
            Err(AuthError::InsufficientScope) => {
                unauthorized_response(&state.config, "insufficient_scope", true)
            }
            Err(AuthError::SubjectNotAllowed)
            | Err(AuthError::Inactive)
            | Err(AuthError::OpenIdConfigUnavailable) => {
                unauthorized_response(&state.config, "invalid_token", false)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    expected_iss = %state.config.authorization_server,
                    expected_aud = %resource,
                    "MCP bearer token rejected"
                );
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
        _ => "valid Bearer access token required",
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
