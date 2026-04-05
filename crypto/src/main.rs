use std::sync::Arc;

use communication::WorkerServiceServer;
use crypto::events::EventBus;
use crypto::grpc::WorkerGrpcServer;
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
use tonic::transport::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/crypto_bot".into());
    let db = Database::connect(&database_url).await?;
    tracing::info!("Connected to database");

    let worker_id = std::env::var("WORKER_ID").unwrap_or_else(|_| "worker-1".into());
    let grpc_addr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into())
        .parse()?;

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

    if let Some(saved) = portfolio.load_latest().await? {
        tracing::info!(balance = %saved.total_balance, "Restored portfolio from database");
    }

    let (settings_svc, settings_rx) = SettingsService::new(db.clone(), BotSettings::default());
    let settings = Arc::new(settings_svc);
    settings.load_or_create().await?;
    tracing::info!("Settings loaded");

    let trading_engine: Box<dyn crypto::llm::DecisionEngine> = Box::new(HeuristicFallback);
    let polymarket_engine: Box<dyn crypto::llm::DecisionEngine> = Box::new(HeuristicFallback);

    let trading_service = Arc::new(TradingService::new(
        db.clone(),
        Box::new(LlmTradingStrategy::new(trading_engine)),
        Box::new(PaperTradeExecutor),
        Arc::clone(&portfolio),
        Arc::clone(&settings),
        Arc::clone(&event_bus),
    ));

    let polymarket_service = Arc::new(PolymarketService::new(
        db.clone(),
        Box::new(LlmPolymarketStrategy::new(polymarket_engine)),
        Box::new(PaperPolymarketExecutor),
        Arc::clone(&portfolio),
        Arc::clone(&settings),
        Arc::clone(&event_bus),
    ));

    // gRPC server
    let grpc_server = WorkerGrpcServer::new(
        worker_id.clone(),
        Arc::clone(&trading_service),
        Arc::clone(&polymarket_service),
        Arc::clone(&portfolio),
        Arc::clone(&settings),
    );

    let grpc_handle = tokio::spawn(async move {
        tracing::info!(%grpc_addr, "gRPC WorkerService listening");
        if let Err(e) = Server::builder()
            .add_service(WorkerServiceServer::new(grpc_server))
            .serve(grpc_addr)
            .await
        {
            tracing::error!(error = %e, "gRPC server failed");
        }
    });

    // Trading loop: wakes on interval OR when settings change
    let trading_handle = {
        let svc = Arc::clone(&trading_service);
        let cfg = Arc::clone(&settings);
        let mut rx = settings_rx.clone();
        tokio::spawn(async move {
            tracing::info!("Trading module started");
            loop {
                let interval_secs = cfg.get().trading_interval_secs;
                let sleep = tokio::time::sleep(std::time::Duration::from_secs(interval_secs));
                tokio::pin!(sleep);

                tokio::select! {
                    () = &mut sleep => {}
                    _ = rx.changed() => {
                        let new_settings = rx.borrow_and_update().clone();
                        tracing::info!("Trading: settings changed, reevaluating positions");
                        if let Err(e) = svc.reevaluate_positions(&new_settings).await {
                            tracing::error!(error = %e, "Failed to reevaluate positions on settings change");
                        }
                        continue;
                    }
                }

                if let Err(e) = svc.update_position_prices().await {
                    tracing::error!(error = %e, "Failed to update position prices");
                }
                if let Err(e) = svc.check_stop_losses().await {
                    tracing::error!(error = %e, "Failed to check stop-losses");
                }
            }
        })
    };

    // Polymarket loop: wakes on interval OR when settings change
    let polymarket_handle = {
        let svc = Arc::clone(&polymarket_service);
        let cfg = Arc::clone(&settings);
        let mut rx = settings_rx.clone();
        tokio::spawn(async move {
            tracing::info!("Polymarket module started");
            loop {
                let interval_secs = cfg.get().polymarket_interval_secs;
                let sleep = tokio::time::sleep(std::time::Duration::from_secs(interval_secs));
                tokio::pin!(sleep);

                tokio::select! {
                    () = &mut sleep => {}
                    _ = rx.changed() => {
                        let new_settings = rx.borrow_and_update().clone();
                        tracing::info!("Polymarket: settings changed, reevaluating predictions");
                        if let Err(e) = svc.reevaluate_predictions(&new_settings).await {
                            tracing::error!(error = %e, "Failed to reevaluate predictions on settings change");
                        }
                        continue;
                    }
                }

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

    tracing::info!(worker_id, "Bot running -- press Ctrl+C to stop");

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received");

    portfolio.persist_snapshot().await?;
    tracing::info!("Final portfolio state persisted");

    trading_handle.abort();
    polymarket_handle.abort();
    snapshot_handle.abort();
    grpc_handle.abort();

    Ok(())
}
