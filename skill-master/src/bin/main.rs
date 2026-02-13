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

    let raw_config = tokio::fs::read_to_string("config.toml")
        .await
        .expect("Failed to read config file");

    let config: skill_master::Config =
        toml::from_str(&raw_config).expect("Failed to parse config");

    let start = std::time::Instant::now();

    let state = skill_master::app_state(config.clone()).await;

    tracing::info!(
        "App state initialized in {:.2?} ms",
        start.elapsed().as_millis()
    );

    let app = skill_master::routes::create_routes(state);

    let addr: std::net::SocketAddr =
        "0.0.0.0:8080".parse().expect("Failed to parse address");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("Starting server on {}", addr);

    axum::serve(listener, app.into_make_service())
        .await
        .expect("Failed to start server");
}
