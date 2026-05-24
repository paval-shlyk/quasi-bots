mod config;
mod oauth;
mod server;
mod tools;

use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Router,
    middleware,
    routing::{get, post},
};
use clap::Parser;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig,
    session::local::LocalSessionManager,
    tower::StreamableHttpService,
};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use finance::portfolio::RestClient;
use oauth::{McpOAuthStore, bearer_auth_middleware};
use server::FinanceMcpServer;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "finance-mcp", about = "MCP server for DZENGI.com exchange API")]
struct Args {
    /// Path to the TOML configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: String,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    let raw = tokio::fs::read_to_string(&args.config)
        .await
        .with_context(|| format!("failed to read config file: {}", args.config))?;

    let cfg: Config =
        toml::from_str(&raw).context("failed to parse config.toml")?;

    let addr: SocketAddr = cfg
        .server
        .addr
        .parse()
        .with_context(|| format!("invalid server.addr: {}", cfg.server.addr))?;

    // ── Build stateless REST client ───────────────────────────────────────────
    let api = Arc::new(RestClient::new(
        &cfg.provider.base_url,
        &cfg.provider.api_key,
        &cfg.provider.api_secret,
    ));

    // ── Build OAuth store ─────────────────────────────────────────────────────
    let oauth_store = Arc::new(McpOAuthStore::new(cfg.server.clone()));

    // ── Build MCP Streamable HTTP service ─────────────────────────────────────
    let mcp_service: StreamableHttpService<FinanceMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            {
                let api = api.clone();
                move || Ok(FinanceMcpServer::new(api.clone()))
            },
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );

    // ── CORS — required so browser-based OAuth callbacks work ─────────────────
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // ── OAuth public routes (no auth needed) ──────────────────────────────────
    let oauth_router = Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth::oauth_metadata),
        )
        .route("/oauth/register",  post(oauth::oauth_register))
        .route("/oauth/authorize", get(oauth::oauth_authorize))
        .route("/oauth/approve",   post(oauth::oauth_approve))
        .route("/oauth/token",     post(oauth::oauth_token))
        .layer(cors)
        .with_state(oauth_store.clone());

    // ── Protected MCP route ───────────────────────────────────────────────────
    let mcp_router = Router::new()
        .nest_service("/mcp", mcp_service)
        .layer(middleware::from_fn_with_state(
            oauth_store.clone(),
            bearer_auth_middleware,
        ));

    let app = Router::new()
        .merge(oauth_router)
        .merge(mcp_router);

    info!("finance-mcp listening on {addr}");
    info!("MCP endpoint: http://{addr}/mcp");
    info!("OAuth metadata: http://{addr}/.well-known/oauth-authorization-server");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            info!("shutting down finance-mcp");
        })
        .await?;

    Ok(())
}
