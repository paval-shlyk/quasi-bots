mod articles;
mod config;
mod links;
mod llm;
mod routes;
mod state;
mod sync_task;
mod topics;

pub use config::*;
pub use state::*;

pub use llm::*;

pub use articles::*;
pub use routes::*;
pub use sync_task::*;

pub async fn connect(
    config: Config,
    pool: sqlx::SqlitePool,
) -> anyhow::Result<NewsState> {
    use std::sync::Arc;

    let state = NewsState {
        gemini_api: llm::GeminiApi::connect(config.gemini_config.clone())
            .await?,
        config: Arc::new(config),
        broken_links: Arc::new(tokio::sync::RwLock::new(vec![])),
        purge_notify: Arc::new(tokio::sync::Notify::new()),
        pool,
    };

    sync_task::spawn(state.clone());

    Ok(state)
}
