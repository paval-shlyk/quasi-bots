pub mod mcp;
pub mod quotes;
pub mod routes;
pub mod search;
pub mod tools;
pub mod version;

mod state;

mod config;
mod middleware;

use std::sync::Arc;

pub use config::*;
pub use state::*;

pub async fn connect_db(db_file: &str) -> sqlx::SqlitePool {
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;

    let db_url = format!("sqlite://{}?mode=rwc", db_file);

    tracing::info!("Connecting to database at {}", db_file);

    let options = SqliteConnectOptions::from_str(&db_url)
        .expect("Invalid sqlite database URL")
        .create_if_missing(true)
        .foreign_keys(true);

    sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .expect("Failed to connect to database")
}

pub async fn apply_migrations(pool: &sqlx::SqlitePool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to apply database migrations");
}

pub async fn app_state(config: Config) -> AppState {
    let metrics_handle = telemetry::init_prometheus_recorder();

    let pool = connect_db(&config.db_file).await;
    apply_migrations(&pool).await;

    let knowledge_state = knowledge::connect(pool.clone())
        .await
        .expect("Failed to connect knowledge database");

    let topics = knowledge::fetch_topics(&pool)
        .await
        .expect("Failed to fetch topics");

    if topics.topics.is_empty() {
        knowledge::refresh_from_files(
            &knowledge_state,
            &config.knowledge.database_file,
        )
        .await
        .expect("Failed to refresh empty knowledge database");
    }

    let finance_state = finance::connect(config.finance.clone(), &pool)
        .await
        .expect("Failed to initialize finance state");

    let news_state = news::connect(config.news.clone(), pool.clone())
        .await
        .expect("Failed to initialize news state");

    telemetry::spawn_system_monitor(15);

    let state = AppState {
        config: Arc::new(config),
        pool,
        needs_more_quotes: Arc::new(tokio::sync::Notify::new()),
        knowledge_state,
        finance_state,
        news_state,
        metrics_handle,
    };

    tokio::task::spawn(crate::quotes::sync_task::task(state.clone()));

    // P1 A3/A4/A4.5: mounted [finance.alerts] / [finance.telegram] with
    // optional TRADING_* / TELEGRAM_* env overrides. Tokens never logged.
    let alerts_file = &state.config.finance.alerts;
    let telegram_resolved = state.config.finance.telegram.resolve();

    if alerts_file.resolve_enabled() {
        let pool = state.finance_state.pool().clone();
        let api = state.finance_state.api().clone();
        let cfg =
            finance::AlertEvaluatorConfig::from_alerts_config(alerts_file);
        tracing::info!("spawning trading alert evaluator (config/env enabled)");
        tokio::task::spawn(finance::run_alert_evaluator(pool, api, cfg));
    } else {
        tracing::debug!(
            "trading alert evaluator idle (finance.alerts.evaluator_enabled or TRADING_ALERT_EVALUATOR=1)"
        );
    }

    // A4: outbox consumer. Single bot_token shared with commands when both on.
    if telegram_resolved.delivery_enabled {
        match finance::TelegramDeliveryConfig::from_resolved(&telegram_resolved)
        {
            Some(cfg) => {
                let pool = state.finance_state.pool().clone();
                tracing::info!(
                    "spawning trading telegram outbox consumer (config/env enabled; token redacted)"
                );
                tokio::task::spawn(finance::run_telegram_outbox_consumer(
                    pool, cfg,
                ));
            }
            None => {
                tracing::warn!(
                    "telegram delivery enabled but bot_token/chat_id missing/empty; consumer not started"
                );
            }
        }
    } else {
        tracing::debug!(
            "trading telegram delivery idle (finance.telegram.delivery_enabled or TRADING_TELEGRAM_DELIVERY=1)"
        );
    }

    // A4.5: inbound commands — sole getUpdates poller for this bot_token.
    if telegram_resolved.commands_enabled {
        match finance::TelegramCommandsConfig::from_resolved(&telegram_resolved)
        {
            Some(cfg) => {
                let pool = state.finance_state.pool().clone();
                let api = state.finance_state.api().clone();
                tracing::info!(
                    "spawning trading telegram commands worker (sole getUpdates; token redacted; no MCP)"
                );
                tokio::task::spawn(finance::run_telegram_commands_worker(
                    pool, api, cfg,
                ));
            }
            None => {
                tracing::warn!(
                    "telegram commands enabled but bot_token/chat_id missing/empty; worker not started"
                );
            }
        }
    } else {
        tracing::debug!(
            "trading telegram commands idle (finance.telegram.commands_enabled or TRADING_TELEGRAM_COMMANDS=1)"
        );
    }

    state
}
