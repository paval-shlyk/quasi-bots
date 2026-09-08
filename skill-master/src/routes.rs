use crate::AppState;
use axum::extract::{Query, State};
use axum::{Json, Router, response::IntoResponse, routing::get};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

pub async fn create_routes(
    state: AppState,
    token: CancellationToken,
) -> Router<()> {
    let mut router = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(get_metrics));

    // Never prod by default: debug builds always on; release needs FINANCE_DEBUG=1.
    if finance::finance_debug_enabled() {
        tracing::warn!(
            "finance debug HTTP routes enabled (/debug/finance/*) — local/FINANCE_DEBUG only"
        );
        router = router
            .route("/debug/finance/ws-topics", get(debug_ws_topics))
            .route("/debug/finance/ticker", get(debug_rest_ticker))
            .route("/debug/finance/ws-frame", get(debug_ws_frame));
    }

    let router = router
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

#[derive(Debug, Deserialize)]
struct DebugSymbolQuery {
    /// Broker / book symbol (e.g. TSLA, Gold, US500).
    #[serde(default = "default_debug_symbol")]
    symbol: String,
}

fn default_debug_symbol() -> String {
    "TSLA".into()
}

async fn debug_ws_topics(State(state): State<AppState>) -> impl IntoResponse {
    let base = &state.finance_state.api().base_url;
    Json(finance::ws_topics(base))
}

async fn debug_rest_ticker(
    State(state): State<AppState>,
    Query(q): Query<DebugSymbolQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    finance::rest_ticker_sample(state.finance_state.api(), &q.symbol)
        .await
        .map(Json)
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))
}

async fn debug_ws_frame(
    State(state): State<AppState>,
    Query(q): Query<DebugSymbolQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    finance::ws_ticker_frame_redacted(state.finance_state.api(), &q.symbol)
        .await
        .map(Json)
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))
}
