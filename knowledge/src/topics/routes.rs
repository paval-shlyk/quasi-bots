use axum::http::StatusCode;
use axum::{Json, extract::State, response::IntoResponse};

use crate::topics::TopicWithStatistics;
use crate::{KnowledgeState, topics};

#[utoipa::path(
    get,
    path = "/knowledge-bank/topics",
    responses(
        (status = 200, description = "All topics retrieved successfully", body = Vec<TopicWithStatistics>),
        (status = 500, description = "Internal server error")
    )
)]
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
