use axum::{
    Json,
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{
    config::McpServerConfig,
    oauth::{SharedOAuthState, store::OAuthStore},
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
            handle_auth_code(store, token_req, &config).await
        }
        "refresh_token" => handle_refresh(store, token_req, &config).await,
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
    let session = store.sessions.write().await.remove(&req.code);
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

    // PKCE S256 verification.
    if let Some(challenge) =
        session.code_challenge.as_deref().filter(|s| !s.is_empty())
    {
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
    }

    let token = store.issue_token(config.token_ttl(), session.scope).await;

    (StatusCode::OK, Json(token)).into_response()
}

async fn handle_refresh(
    store: &OAuthStore,
    req: Token,
    config: &McpServerConfig,
) -> Response {
    let old_key = {
        let tokens = store.tokens.read().await;
        tokens
            .iter()
            .find(|(_, v)| v.refresh_token == req.refresh_token)
            .map(|(k, _)| k.clone())
    };

    let old = match old_key {
        Some(k) => store.tokens.write().await.remove(&k),
        None => None,
    };

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
            let token = store.issue_token(config.token_ttl(), prev.scope).await;

            (StatusCode::OK, Json(token)).into_response()
        }
    }
}

/// Returns true when SHA-256(verifier) base64url-no-pad == challenge.
fn pkce_s256_matches(verifier: &str, challenge: &str) -> bool {
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(verifier.as_bytes());

    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest) == challenge
}
