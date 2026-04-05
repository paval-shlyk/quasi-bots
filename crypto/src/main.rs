use std::sync::Arc;

use crypto::events::EventBus;
use crypto::llm::HeuristicFallback;
use crypto::polymarket::{
    executor::PaperPolymarketExecutor, service::PolymarketService,
    strategy::LlmPolymarketStrategy,
};
use crypto::portfolio::{PortfolioService, PortfolioState};
use crypto::settings::{BotSettings, SettingsService};
use crypto::trading::{
    executor::PaperTradeExecutor, service::TradingService, strategy::LlmTradingStrategy,
};
use rust_decimal::Decimal;
use sea_orm::Database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // ---- database ---------------------------------------------------------
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/crypto_bot".into());
    let db = Database::connect(&database_url).await?;
    tracing::info!("Connected to database");

    // ---- shared services --------------------------------------------------
    let event_bus = Arc::new(EventBus::default());

    let initial_balance = std::env::var("INITIAL_BALANCE")
        .ok()
        .and_then(|v| v.parse::<Decimal>().ok())
        .unwrap_or(Decimal::new(10_000, 0));

    let portfolio = Arc::new(PortfolioService::new(
        db.clone(),
        PortfolioState {
            total_balance: initial_balance,
            available_balance: initial_balance,
            ..Default::default()
        },
    ));

    // Restore from DB if a prior snapshot exists
    if let Some(saved) = portfolio.load_latest().await? {
        tracing::info!(balance = %saved.total_balance, "Restored portfolio from database");
    }

    let settings = Arc::new(SettingsService::new(db.clone(), BotSettings::default()));
    settings.load_or_create().await?;
    tracing::info!("Settings loaded");

    // ---- LLM decision engines (fallback until rig-core is wired) ----------
    let trading_engine: Box<dyn crypto::llm::DecisionEngine> = Box::new(HeuristicFallback);
    let polymarket_engine: Box<dyn crypto::llm::DecisionEngine> = Box::new(HeuristicFallback);

    // ---- trading module ---------------------------------------------------
    let trading_service = Arc::new(TradingService::new(
        db.clone(),
        Box::new(LlmTradingStrategy::new(trading_engine)),
        Box::new(PaperTradeExecutor),
        Arc::clone(&portfolio),
        Arc::clone(&settings),
        Arc::clone(&event_bus),
    ));

    // ---- polymarket module ------------------------------------------------
    let polymarket_service = Arc::new(PolymarketService::new(
        db.clone(),
        Box::new(LlmPolymarketStrategy::new(polymarket_engine)),
        Box::new(PaperPolymarketExecutor),
        Arc::clone(&portfolio),
        Arc::clone(&settings),
        Arc::clone(&event_bus),
    ));

    // ---- spawn parallel module tasks --------------------------------------
    let trading_handle = {
        let svc = Arc::clone(&trading_service);
        let cfg = Arc::clone(&settings);
        tokio::spawn(async move {
            tracing::info!("Trading module started");
            loop {
                let s = cfg.get().await;
                tokio::time::sleep(std::time::Duration::from_secs(s.trading_interval_secs)).await;

                if let Err(e) = svc.update_position_prices().await {
                    tracing::error!(error = %e, "Failed to update position prices");
                }
                if let Err(e) = svc.check_stop_losses().await {
                    tracing::error!(error = %e, "Failed to check stop-losses");
                }
                // In production: receive MarketData from Barter WS → call svc.tick()
            }
        })
    };

    let polymarket_handle = {
        let svc = Arc::clone(&polymarket_service);
        let cfg = Arc::clone(&settings);
        tokio::spawn(async move {
            tracing::info!("Polymarket module started");
            loop {
                let s = cfg.get().await;
                tokio::time::sleep(std::time::Duration::from_secs(s.polymarket_interval_secs))
                    .await;

                if let Err(e) = svc.tick().await {
                    tracing::error!(error = %e, "Polymarket tick failed");
                }
            }
        })
    };

    let snapshot_handle = {
        let p = Arc::clone(&portfolio);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                if let Err(e) = p.persist_snapshot().await {
                    tracing::error!(error = %e, "Failed to persist portfolio snapshot");
                }
            }
        })
    };

    tracing::info!("Bot running — press Ctrl+C to stop");

    // ---- graceful shutdown ------------------------------------------------
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received");

    portfolio.persist_snapshot().await?;
    tracing::info!("Final portfolio state persisted");

    trading_handle.abort();
    polymarket_handle.abort();
    snapshot_handle.abort();

    Ok(())
}
