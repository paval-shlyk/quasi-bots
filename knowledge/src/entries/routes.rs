use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

use crate::{KnowledgeState, entries};

#[derive(Debug, serde::Deserialize)]
pub struct NewKnowledge {
    pub topic_id: u64,
    pub question: String,
    pub truth: String,
    pub tags: Vec<String>,
}

/// Add new knowledge entry where you can add this
pub async fn post_new_knowledge(
    State(_state): State<KnowledgeState>,
    Json(_body): Json<NewKnowledge>,
) -> impl IntoResponse {
}

#[derive(serde::Deserialize)]
pub struct QuestionBody {
    pub tag: Option<String>,
}

#[axum::debug_handler]
pub async fn post_next_daily_question(
    State(state): State<KnowledgeState>,
) -> impl IntoResponse {
    match entries::fetch_random_entry(&state.pool).await {
        Ok(entry) => (StatusCode::OK, Json(entry)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
