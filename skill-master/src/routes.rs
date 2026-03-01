use crate::openapi::ApiDoc;
use crate::{AppState, quotes, search};
use axum::extract::State;
use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

pub async fn get_openapi_json() -> impl IntoResponse {
    axum::http::Response::builder()
        .header("Content-Type", "application/json")
        .body(
            ApiDoc::openapi()
                .to_json()
                .expect("Failed to serialize OpenAPI spec"),
        )
        .expect("Failed to build response")
}

#[rustfmt::skip]
pub fn create_routes(state: AppState) -> Router<()> {
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
    
    let finance_routes = Router::new()
        .route(
            "/portfolio",
            get(finance::get_portfolio),
        ).with_state(state.finance_state.clone());

    let expenses_routes = finance::expenses::router()
        .with_state(state.finance_state.clone());

    let news_routes = Router::new()
        .route("/today", get(news::get_today_news))
        .route("/broken-links", get(news::get_broken_links))
        .route("/sources", get(news::get_source_statistics))
        .route(
            "/topics",
            get(news::get_chosen_topics).post(news::post_chosen_topic),
        )
        .with_state(state.news_state.clone());

    Router::new()
        .merge(Scalar::with_url("/scalar", ApiDoc::openapi()))
        .nest("/knowledge-bank", knowledge_routes)
        .nest("/market-tracker", finance_routes)
        .nest("/news-bank", news_routes)
        .nest("/expenses-bank", expenses_routes)

        .route("/openapi.json", get(get_openapi_json))
        .route("/metrics", get(get_metrics))
        .layer(axum::middleware::from_fn(crate::middleware::track_http))
        .with_state(state.clone())
        .route("/health", get(health_check))


        .route("/search", get(search::get_search))

        .route("/quotes-bank/authors", get(quotes::get_known_authors))
        .route("/quotes-bank/next", post(quotes::post_next_unused_quote))

        .with_state(state.clone())
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics_handle.render()
}
