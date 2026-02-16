use axum::http::StatusCode;
use axum::{Json, extract::State, response::IntoResponse};

use crate::entries::HumanEntry;
use crate::{KnowledgeState, entries};

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct NewKnowledge {
    pub topic_id: u64,
    pub question: String,
    pub truth: String,
    pub tags: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/knowledge-bank/entries",
    request_body = NewKnowledge,
    responses(
        (status = 201, description = "Knowledge entry added successfully"),
        (status = 500, description = "Internal server error")
    ),
)]
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

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct QuestionBody {
    pub tag: Option<String>,
}

#[utoipa::path(
    post,
    path = "/knowledge-bank/next",
    responses(
        (status = 200, description = "Next daily question retrieved successfully", body = HumanEntry),
        (status = 500, description = "Internal server error")
    )
)]
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
