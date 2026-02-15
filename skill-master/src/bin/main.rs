use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: String,
    /// Port to run the server on
    #[arg(short, long, default_value = "8080")]
    port: u16,
}

#[tokio::main]
pub async fn main() {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let raw_config = tokio::fs::read_to_string(&args.config)
        .await
        .unwrap_or_else(|_| {
            panic!("Failed to read config file at {}", args.config)
        });

    let config: skill_master::Config =
        toml::from_str(&raw_config).expect("Failed to parse config");

    let start = std::time::Instant::now();

    let state = skill_master::app_state(config.clone()).await;

    let version = env!("CARGO_PKG_VERSION");
    let git_commit = option_env!("GIT_COMMIT").unwrap_or("unknown");

    tracing::info!(
        "App state initialized in {:.2?} ms. Version: {}, Commit: {}",
        start.elapsed().as_millis(),
        version,
        git_commit
    );

    let app = skill_master::routes::create_routes(state);

    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", args.port)
        .parse()
        .expect("Failed to parse address");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("Starting server on {}", addr);

    axum::serve(listener, app.into_make_service())
        .await
        .expect("Failed to start server");
}
