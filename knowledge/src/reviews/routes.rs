use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use reqwest::StatusCode;

use crate::{KnowledgeState, reviews};

#[derive(Debug, serde::Deserialize)]
pub struct ReviewAttempts {
    pub attempts: u32,
}

#[derive(Debug, serde::Deserialize)]
pub struct ReviewQuery {
    pub days: Option<u32>,
}

pub async fn get_recent_reviews(
    State(state): State<KnowledgeState>,
    query: Query<ReviewQuery>,
) -> impl IntoResponse {
    reviews::fetch_recent_reviews(&state.pool, query.days)
        .await
        .map(|reviews| (StatusCode::OK, Json(reviews)).into_response())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch recent reviews: {}", e),
            )
                .into_response()
        })
}

pub async fn post_entry_review(
    State(state): State<KnowledgeState>,
    Path(entry_name): Path<String>,
    Json(body): Json<ReviewAttempts>,
) -> impl IntoResponse {
    reviews::update_review(&state.pool, entry_name, body.attempts as i32)
        .await
        .map(|()| StatusCode::OK)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update review: {}", e),
            )
        })
}
