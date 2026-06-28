use axum::{
    Json,
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{
    config::McpServerConfig,
    oauth::{SharedOAuthState, metadata, store::OAuthStore},
};

#[derive(Debug, serde::Deserialize)]
pub struct Token {
    pub grant_type: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub redirect_uri: String,
    #[serde(default)]
    pub code_verifier: Option<String>,
    #[serde(default)]
    pub refresh_token: String,
    /// RFC 8707 resource indicator.
    #[serde(default)]
    pub resource: Option<String>,
    /// Client identifier (required for public clients / code exchange).
    #[serde(default)]
    pub client_id: Option<String>,
}

//  POST /oauth/token
pub async fn authorize_or_refresh_token(
    State(state): State<SharedOAuthState>,
    request: Request<Body>,
) -> impl IntoResponse {
    let config = &state.config;
    let store = &state.store;

    let bytes =
        match axum::body::to_bytes(request.into_body(), usize::MAX).await {
            Ok(b) => b,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":"invalid_request"})),
                )
                    .into_response();
            }
        };

    let token_req: Token = if let Ok(f) = serde_urlencoded::from_bytes(&bytes) {
        f
    } else if let Ok(j) = serde_json::from_slice(&bytes) {
        j
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_request",
                "error_description": "could not parse request body"
            })),
        )
            .into_response();
    };

    match token_req.grant_type.as_str() {
        "authorization_code" => {
            handle_auth_code(store, token_req, config).await
        }
        "refresh_token" => handle_refresh(store, token_req, config).await,
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"unsupported_grant_type"})),
        )
            .into_response(),
    }
}

async fn handle_auth_code(
    store: &OAuthStore,
    req: Token,
    config: &McpServerConfig,
) -> Response {
    if let Some(resource) = req.resource.as_deref() {
        if !metadata::resource_matches(config, resource) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_target",
                    "error_description": "resource parameter mismatch"
                })),
            )
                .into_response();
        }
    }

    let session = store.take_session(&req.code).await;
    let session = match session {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "unknown or expired code"
                })),
            )
                .into_response();
        }
    };

    if session.redirect_uri != req.redirect_uri {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "redirect_uri mismatch"
            })),
        )
            .into_response();
    }

    // Bind the authorization code to the client that started the flow (RFC 6749 §4.1.3, OAuth 2.1 security).
    let client_id = match req.client_id.as_deref() {
        Some(cid) if !cid.is_empty() => cid,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "client_id required"
                })),
            )
                .into_response();
        }
    };
    if client_id != session.client_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "client_id mismatch"
            })),
        )
            .into_response();
    }

    if let Some(resource) = req.resource.as_deref() {
        if let Some(session_resource) = session.resource.as_deref() {
            if !metadata::resource_matches(config, session_resource)
                || normalize(resource) != normalize(session_resource)
            {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": "resource parameter mismatch with authorization session"
                    })),
                )
                    .into_response();
            }
        }
    }

    let challenge = match session
        .code_challenge
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        Some(c) => c,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "PKCE code_challenge missing from session"
                })),
            )
                .into_response();
        }
    };

    match req.code_verifier.as_deref() {
        Some(verifier) if pkce_s256_matches(verifier, challenge) => {}
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "code_verifier mismatch"
                })),
            )
                .into_response();
        }
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "code_verifier required"
                })),
            )
                .into_response();
        }
    }

    let token = store
        .issue_token(
            config.token_ttl(),
            session.scope,
            Some(config.issuer_url()),
            session.owner_sub,
            Some(session.client_id.clone()),
        )
        .await;

    (StatusCode::OK, Json(token)).into_response()
}

async fn handle_refresh(
    store: &OAuthStore,
    req: Token,
    config: &McpServerConfig,
) -> Response {
    // For public clients, client_id must be provided and must match the token's bound client.
    let Some(client_id) = req.client_id.as_ref().filter(|cid| !cid.is_empty())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "client_id required"
            })),
        )
            .into_response();
    };

    let old = store.take_token_by_refresh(&req.refresh_token).await;

    match old {
        None => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "unknown refresh_token"
            })),
        )
            .into_response(),
        Some(prev) => {
            if prev.client_id.as_deref() != Some(client_id) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": "client_id mismatch"
                    })),
                )
                    .into_response();
            }

            let token = store
                .issue_token(
                    config.token_ttl(),
                    prev.scope,
                    Some(config.issuer_url()),
                    prev.owner_sub,
                    prev.client_id.clone(),
                )
                .await;

            (StatusCode::OK, Json(token)).into_response()
        }
    }
}

fn normalize(uri: &str) -> String {
    uri.trim_end_matches('/').to_lowercase()
}

/// Returns true when SHA-256(verifier) base64url-no-pad == challenge.
fn pkce_s256_matches(verifier: &str, challenge: &str) -> bool {
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(verifier.as_bytes());

    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest) == challenge
}
