use crate::AppState;
use axum::extract::State;
use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};

pub async fn create_routes(state: AppState) -> Router<()> {
    let router = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(get_metrics))
        .layer(axum::middleware::from_fn(crate::middleware::track_http))
        .with_state(state.clone());

    let oauth_state = crate::mcp::oauth_state(&state.config.mcp)
        .await
        .expect("failed to initialize MCP OAuth state");
    let mcp_router = crate::mcp::mount(
        state.clone(),
        oauth_state.clone(),
        &state.config.mcp,
        tokio_util::sync::CancellationToken::new(),
    );

    Router::new()
        .merge(router)
        .merge(mcp_auth::oauth::router().with_state(oauth_state))
        .merge(mcp_router)
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics_handle.render()
}
