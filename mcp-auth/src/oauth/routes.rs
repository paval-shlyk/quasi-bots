use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

use crate::oauth::metadata::ProtectedResourceMetadata;

use super::SharedOAuthState;

// RFC 9728
pub async fn protected_resource_metadata(
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    let meta = ProtectedResourceMetadata::from_config(&state.config);
    (StatusCode::OK, Json(meta))
}
