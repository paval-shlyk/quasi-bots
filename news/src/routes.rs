use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

use crate::{ArticlesWithTopic, NewsState, RssSource, articles, links, topics};

#[utoipa::path(
    post,
    path = "/news-bank/topics",
    tag = "News",
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
    tag = "News",
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

#[utoipa::path(
    get,
    path = "/news-bank/broken-links",
    tag = "News",
    responses(
        (status = 200, description = "Broken links retrieved successfully", body = Vec<crate::links::BrokenLink>),
        (status = 500, description = "Internal server error")
    )
)]
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
    tag = "News",
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

#[utoipa::path(
    get,
    path = "/news-bank/sources",
    tag = "News",
    responses(
        (status = 200, description = "Source statistics retrieved successfully", body = Vec<crate::links::SourceStatistics>),
        (status = 500, description = "Internal server error")
    )
)]
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
