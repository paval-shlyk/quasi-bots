use crate::AppState;
use axum::extract::State;
use axum::{Json, Router, response::IntoResponse, routing::get};
use tokio_util::sync::CancellationToken;

pub async fn create_routes(
    state: AppState,
    token: CancellationToken,
) -> Router<()> {
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
        token,
    );

    Router::new()
        .merge(router)
        .merge(mcp_auth::oauth::router().with_state(oauth_state))
        .merge(mcp_router)
}

async fn health_check() -> impl IntoResponse {
    Json(crate::version::health_info())
}

async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics_handle.render()
}
