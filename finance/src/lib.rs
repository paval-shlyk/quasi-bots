pub mod metrics;
pub mod model;
mod recommendations;
mod routes;
mod state;

pub use routes::*;
pub use state::FinanceState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    //dummy rss source
    pub rss_source: reqwest::Url,
}

pub async fn connect(
    config: Config,
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<FinanceState> {
    Ok(FinanceState {
        pool: pool.clone(),
        config: std::sync::Arc::new(config),
    })
}
