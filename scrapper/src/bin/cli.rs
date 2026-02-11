use std::sync::Arc;

use tokio::sync::Notify;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
pub async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize database
    let db_url = "sqlite://scrapper.db?mode=rwc";
    let pool = scrapper::connect_db(db_url).await;

    let raw_config = tokio::fs::read_to_string("config.toml")
        .await
        .expect("Failed to read config file");

    let config: scrapper::config::Config =
        toml::from_str(&raw_config).expect("Failed to parse config");

    let state = Arc::new(scrapper::routes::AppState {
        config: Arc::new(config),
        pool,
        needs_more_quotes: Arc::new(Notify::new()),
    });

    let app = scrapper::routes::create_routes(state);

    let addr: std::net::SocketAddr =
        "0.0.0.0:8080".parse().expect("Failed to parse address");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("Starting server on {}", addr);

    axum::serve(listener, app.into_make_service())
        .await
        .expect("Failed to start server");
}
