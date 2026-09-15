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

    // P1 A3: Dzengi WS alert evaluator (outbox only; Telegram is A4).
    // Gate: TRADING_ALERT_EVALUATOR=1|true|yes
    if finance::alert_evaluator_enabled() {
        let pool = state.finance_state.pool().clone();
        let api = state.finance_state.api().clone();
        let cfg = finance::AlertEvaluatorConfig::from_env();
        tracing::info!(
            "spawning trading alert evaluator (feature env enabled)"
        );
        tokio::task::spawn(finance::run_alert_evaluator(pool, api, cfg));
    } else {
        tracing::debug!(
            "trading alert evaluator idle (set TRADING_ALERT_EVALUATOR=1 to enable)"
        );
    }

    state
}
