mod config;
mod oauth;
mod server;

use std::net::SocketAddr;

use anyhow::{Context, Result};
use axum::{Router, middleware};
use clap::Parser;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, session::local::LocalSessionManager,
    tower::StreamableHttpService,
};
use tracing::info;
use tracing_subscriber::{
    EnvFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::{config::McpServerConfig, server::HelloWorldMcpServer};

#[derive(Parser)]
#[command(
    name = "finance-mcp",
    about = "MCP server for DZENGI.com exchange API"
)]
struct Args {
    /// Path to the TOML configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: String,
}

fn create_routes(cfg: McpServerConfig) -> Router<()> {
    let mcp_service = StreamableHttpService::new(
        move || Ok(HelloWorldMcpServer::new()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    let oauth_state = oauth::state(cfg);

    let oauth_router = oauth::router();

    // ── Protected MCP route ───────────────────────────────────────────────────
    let mcp_router = Router::new().nest_service("/mcp", mcp_service).layer(
        middleware::from_fn_with_state(
            oauth_state.clone(),
            oauth::bearer_auth_middleware,
        ),
    );

    Router::new()
        .merge(oauth_router)
        .merge(mcp_router)
        .with_state(oauth_state.clone())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    let raw =
        tokio::fs::read_to_string(&args.config)
            .await
            .with_context(|| {
                format!("failed to read config file: {}", args.config)
            })?;

    let cfg: McpServerConfig =
        toml::from_str(&raw).context("failed to parse config.toml")?;

    let addr: SocketAddr = cfg
        .addr
        .parse()
        .with_context(|| format!("invalid server.addr: {}", cfg.addr))?;

    info!("finance-mcp listening on {addr}");
    info!("MCP endpoint: http://{addr}/mcp");
    info!(
        "OAuth metadata: http://{addr}/.well-known/oauth-authorization-server"
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let app = create_routes(cfg);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            info!("shutting down finance-mcp");
        })
        .await?;

    Ok(())
}
