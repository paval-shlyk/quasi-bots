use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

use crate::{KnowledgeState, topics};

pub async fn get_all_topics(
    State(state): State<KnowledgeState>,
) -> impl IntoResponse {
    match topics::fetch_topics(&state.pool).await {
        Ok(topics) => (StatusCode::OK, Json(topics)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
