use crate::finance;
use crate::{AppState, news, quotes, search};
use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};

#[rustfmt::skip]
pub fn create_routes(state: AppState) -> Router<()> {
    tokio::task::spawn(crate::quotes::sync_task::task(state.clone()));
    let knowledge_routes = Router::new()
                .route("/next", post(knowledge::post_next_daily_question))
                .route("/topics", get(knowledge::get_all_topics))
                .route("/tags", get(knowledge::get_all_tags))
                .route(
                    "/topics/{topic_id}/affinity",
                    post(knowledge::post_topic_affinity),
                )
                .route("/entries", post(knowledge::post_new_knowledge))
                .route(
                    "/entries/{entry_id}/affinity",
                    post(knowledge::post_entry_affinity),
                )
                .route(
                    "/entries/{entry_id}/reviews",
                    post(knowledge::post_entry_review),
                )
                .route("/reviews", get(knowledge::get_recent_reviews))
        .with_state(state.knowledge_state.clone());

    Router::new()
        .nest("/knowledge-bank", knowledge_routes)

        .with_state(state.clone())
        .route("/health", get(health_check))

        .route("/news-bank/today", get(news::get_today_news))
        .route(
            "/news-bank/topics",
            get(news::get_chosen_topics).post(news::post_chosen_topic),
        )

        .route("/search", get(search::get_search))

        .route("/quotes-bank/authors", get(quotes::get_known_authors))
        .route("/quotes-bank/next", post(quotes::post_next_unused_quote))


        .route("/market-tracker/report", get(finance::get_report))
        .route(
            "/market-tracker/recommendations",
            get(finance::get_market_recommendations),
        )
        .with_state(state.clone())
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
