mod articles;
mod config;
mod links;
mod llm;
mod state;
mod sync_task;
mod topics;

use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

pub use config::*;
pub use state::*;

pub use llm::*;

pub use articles::*;
pub use sync_task::*;

pub async fn connect(
    config: Config,
    pool: sqlx::SqlitePool,
) -> anyhow::Result<NewsState> {
    use std::sync::Arc;

    let state = NewsState {
        gemini_api: llm::GeminiApi::connect(config.gemini_config.clone())
            .await?,
        config: Arc::new(config),
        broken_links: Arc::new(tokio::sync::RwLock::new(vec![])),
        purge_notify: Arc::new(tokio::sync::Notify::new()),
        pool,
    };

    sync_task::spawn(state.clone());

    Ok(state)
}

#[derive(Debug, Clone, Hash)]
pub struct Entry {
    pub title: String,
    pub source: String,
    pub authors: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/news-bank/topics",
    responses(
        (status = 501, description = "Not implemented")
    )
)]
pub async fn post_chosen_topic(
    State(_state): State<NewsState>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Topic added")
}

#[utoipa::path(
    get,
    path = "/news-bank/topics",
    responses(
        (status = 200, description = "Topics retrieved successfully", body = Vec<RssSource>)
    )
)]
pub async fn get_chosen_topics(
    State(state): State<NewsState>,
) -> impl IntoResponse {
    topics::select_news_topics(&state.pool)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::warn!("Failed to fetch topics: {e}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "error": e.to_string(),
                })
                .to_string(),
            )
                .into_response()
        })
}

pub async fn get_broken_links(
    State(state): State<NewsState>,
) -> impl IntoResponse {
    links::fetch_broken_links(&state)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::warn!("Failed to fetch broken links: {e}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "error": e.to_string(),
                })
                .to_string(),
            )
                .into_response()
        })
}

#[utoipa::path(
    get,
    path = "/news-bank/today",
    responses(
        (status = 200, description = "Today's news retrieved successfully", body = Vec<ArticlesWithTopic>)
    )
)]
pub async fn get_today_news(
    State(state): State<NewsState>,
) -> impl IntoResponse {
    articles::select_today_articles(&state.pool)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::warn!("Failed to fetch today's news: {e}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "error": e.to_string(),
                })
                .to_string(),
            )
                .into_response()
        })
}

pub async fn get_source_statistics(
    State(state): State<NewsState>,
) -> impl IntoResponse {
    links::select_source_with_statistics(&state.pool)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::warn!("Failed to fetch source statistics: {e}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "error": e.to_string(),
                })
                .to_string(),
            )
                .into_response()
        })
}
