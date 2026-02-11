use std::sync::Arc;

use crate::{config::Config, finance};
use crate::{news, quotes, search};
use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: sqlx::SqlitePool,
    pub needs_more_quotes: Arc<tokio::sync::Notify>,
}

pub fn create_routes(state: Arc<AppState>) -> Router {
    tokio::task::spawn(crate::quotes::sync_task::task(state.clone()));

    Router::new()
        .route("/health", get(health_check))
        .route("/news-bank/today", get(news::get_today_news))
        .route(
            "/news-bank/topics",
            get(news::get_chosen_topics).post(news::post_chosen_topic),
        )
        .route("/search", get(search::get_search))
        .route("/quotes-bank/authors", get(quotes::get_known_authors))
        .route("/quotes-bank/next", post(quotes::post_next_unused_quote))
        .route("/market-tracker/report", get(finance::routes::get_report))
        // .route(
        //     "/market-tracker/recommendations",
        //     get(finance::routes::handler_recommendations),
        // )
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
