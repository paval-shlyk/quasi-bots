use std::sync::Arc;

use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

use crate::AppState;

pub struct NewKnowledge {}

/// Add new knowledge entry where you can add this
pub async fn post_new_knowledge(
    State(_state): State<Arc<AppState>>,
) -> impl IntoResponse {
}

pub async fn get_all_topics(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.knowledge_database.read().await.fetch_topics().await {
        Ok(topics) => (StatusCode::OK, Json(topics)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn post_next_daily_question(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::info!("Next POST");

    match state.knowledge_database.write().await.next_knowledge() {
        Ok(entry) => (StatusCode::OK, Json(entry)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
