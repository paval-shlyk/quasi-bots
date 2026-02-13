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
    State(state): State<KnowledgeState>,
    Json(body): Json<NewKnowledge>,
) -> impl IntoResponse {
    entries::add_new_entry(
        &state.pool,
        body.topic_id,
        body.question,
        body.truth,
        body.tags,
    )
    .await
    .map(|()| StatusCode::CREATED)
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to add new knowledge: {}", e),
        )
    })
}

#[derive(serde::Deserialize)]
pub struct QuestionBody {
    pub tag: Option<String>,
}

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
