pub mod server;

use axum::{Router, middleware};
use mcp_auth::McpAuthConfig;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, session::local::LocalSessionManager,
    tower::StreamableHttpService,
};
use tokio_util::sync::CancellationToken;

use crate::AppState;

use self::server::SkillMasterMcpServer;

pub fn mount(
    app_state: AppState,
    oauth_state: mcp_auth::oauth::SharedOAuthState,
    config: &McpAuthConfig,
    cancel: CancellationToken,
) -> Router<()> {
    let mut http_cfg = StreamableHttpServerConfig::default()
        .with_allowed_hosts(config.allowed_hosts())
        .with_cancellation_token(cancel)
        .with_stateful_mode(config.stateful_mode)
        .with_json_response(config.json_response);

    if !config.allowed_origins.is_empty() {
        http_cfg =
            http_cfg.with_allowed_origins(config.allowed_origins.clone());
    }

    let state = app_state.clone();
    let mcp_service = StreamableHttpService::new(
        move || Ok(SkillMasterMcpServer::new(state.clone())),
        LocalSessionManager::default().into(),
        http_cfg,
    );

    Router::new().nest_service("/mcp", mcp_service).layer(
        middleware::from_fn_with_state(
            oauth_state,
            mcp_auth::oauth::bearer_auth_middleware,
        ),
    )
}

pub async fn oauth_state(
    config: &McpAuthConfig,
) -> anyhow::Result<mcp_auth::oauth::SharedOAuthState> {
    mcp_auth::oauth::state(config.clone()).await
}
