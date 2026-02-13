use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use reqwest::StatusCode;

use crate::{KnowledgeState, affinity};

#[derive(Debug, serde::Deserialize)]
pub struct Affinity {
    /// Number of days until the next review, if the user has an affinity for this topic/entry,
    /// otherwise None
    pub days: u32,
}

pub async fn post_topic_affinity(
    State(state): State<KnowledgeState>,
    Path(topic_id): Path<u64>,
    Json(body): Json<Affinity>,
) -> impl IntoResponse {
    affinity::set_topic_affinity(topic_id, body.days, &state.pool)
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
    State(state): State<KnowledgeState>,
    Path(name): Path<String>,
    Json(body): Json<Affinity>,
) -> impl IntoResponse {
    affinity::set_entry_affinity(name, body.days, &state.pool)
        .await
        .map(|()| StatusCode::OK)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set entry affinity: {}", e),
            )
        })
}
