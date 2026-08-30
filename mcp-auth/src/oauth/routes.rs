use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

use crate::oauth::metadata::{
    AuthorizationServerMetadata, ProtectedResourceMetadata,
};

use super::SharedOAuthState;

// RFC 9728
pub async fn protected_resource_metadata(
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    let meta = ProtectedResourceMetadata::from_config(&state.config);
    (StatusCode::OK, Json(meta))
}

// RFC 8414 — Zitadel has no oauth-authorization-server well-known; MCP agents
// probe the resource origin. Endpoints stay on the external AS.
pub async fn authorization_server_metadata(
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    let Some(oidc) = state.openid_config.read().await.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "temporarily_unavailable",
                "error_description": "authorization server metadata is not available yet",
            })),
        )
            .into_response();
    };

    let meta = AuthorizationServerMetadata::from_openid(&oidc, &state.config);
    (StatusCode::OK, Json(meta)).into_response()
}
