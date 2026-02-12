use std::sync::Arc;

use axum::{extract::State, response::IntoResponse};

use crate::AppState;

pub struct NewKnowledge {}

/// Add new knowledge entry where you can add this
pub async fn post_new_knowledge(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Your implementation for posting new knowledge
}

pub async fn get_all_topics() -> impl IntoResponse {}

pub async fn post_next_daily_question() -> impl IntoResponse {}
