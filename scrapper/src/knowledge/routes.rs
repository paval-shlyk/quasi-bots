use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use reqwest::StatusCode;

use crate::{AppState, knowledge};

#[derive(Debug, serde::Deserialize)]
pub struct NewKnowledge {
    pub topic_id: u64,
    pub question: String,
    pub truth: String,
    pub tags: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Affinity {
    /// Number of days until the next review, if the user has an affinity for this topic/entry,
    /// otherwise None
    pub days: u32,
}

#[derive(Debug, serde::Deserialize)]
pub struct ReviewAttempts {
    pub attempts: u32,
}

#[derive(Debug, serde::Deserialize)]
pub struct ReviewQuery {
    pub days: Option<u32>,
}

/// Add new knowledge entry where you can add this
pub async fn post_new_knowledge(
    State(_state): State<Arc<AppState>>,
    Json(_body): Json<NewKnowledge>,
) -> impl IntoResponse {
}

pub async fn post_topic_affinity(
    State(state): State<Arc<AppState>>,
    Path(topic_id): Path<u64>,
    Json(body): Json<Affinity>,
) -> impl IntoResponse {
    knowledge::set_topic_affinity(
        topic_id,
        body.days,
        &state.knowledge_database.pool,
    )
    .await
    .map(|()| StatusCode::OK)
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to set topic affinity: {}", e),
        )
    })
}

pub async fn post_entry_affinity(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<Affinity>,
) -> impl IntoResponse {
    knowledge::set_entry_affinity(
        name,
        body.days,
        &state.knowledge_database.pool,
    )
    .await
    .map(|()| StatusCode::OK)
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to set entry affinity: {}", e),
        )
    })
}

pub async fn get_recent_reviews(
    State(state): State<Arc<AppState>>,
    query: Query<ReviewQuery>,
) -> impl IntoResponse {
    knowledge::fetch_recent_reviews(&state.pool, query.days)
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
    State(state): State<Arc<AppState>>,
    Path(entry_name): Path<String>,
    Json(body): Json<ReviewAttempts>,
) -> impl IntoResponse {
    knowledge::update_review(&state.pool, entry_name, body.attempts as i32)
        .await
        .map(|()| StatusCode::OK)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update review: {}", e),
            )
        })
}

pub async fn get_all_topics(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.knowledge_database.fetch_topics().await {
        Ok(topics) => (StatusCode::OK, Json(topics)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_all_tags(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.knowledge_database.fetch_tags().await {
        Ok(tags) => (StatusCode::OK, Json(tags)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct QuestionBody {
    pub tag: Option<String>,
}

#[axum::debug_handler]
pub async fn post_next_daily_question(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.knowledge_database.next_knowledge().await {
        Ok(entry) => (StatusCode::OK, Json(entry)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
