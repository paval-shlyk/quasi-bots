use std::sync::Arc;

use crate::{AppState, news, quotes, search};
use crate::{finance, knowledge};
use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};

#[rustfmt::skip]
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

        .route("/knowledge-bank/next", post(knowledge::post_next_daily_question))
        .route("/knowledge-bank/topics", get(knowledge::get_all_topics))
        .route(
            "/knowledge-bank/topics/{topic_id}/affinity",
            post(knowledge::post_topic_affinity)
        )
        .route("/knowledge-bank/entries", post(knowledge::post_new_knowledge))
        .route("/knowledge-bank/entries/{entry_id}/affinity", 
            post(knowledge::post_entry_affinity)
        )
        .route("/knowledge-bank/entries/{entry_id}/reviews", post(knowledge::post_entry_review))
        .route("/knowledge-bank/reviews", post(knowledge::get_recent_reviews))


        .route("/market-tracker/report", get(finance::get_report))
        .route(
            "/market-tracker/recommendations",
            get(finance::get_market_recommendations),
        )
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
