use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

use crate::{KnowledgeState, tags};

pub async fn get_all_tags(
    State(state): State<KnowledgeState>,
) -> impl IntoResponse {
    match tags::fetch_tags(&state.pool).await {
        Ok(tags) => (StatusCode::OK, Json(tags)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
