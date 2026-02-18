mod expenses;
pub mod metrics;
pub mod model;
pub mod portfolio;
mod recommendations;
mod routes;
mod state;

pub use routes::*;
pub use state::FinanceState;

pub use portfolio::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    //dummy rss source
    pub rss_source: reqwest::Url,

    pub provider: portfolio::ProviderConfig,
}

pub async fn connect(
    config: Config,
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<FinanceState> {
    let api = portfolio::RestClient::new(
        config.provider.base_url.clone(),
        config.provider.api_key.clone(),
        config.provider.api_secret.clone(),
    );

    Ok(FinanceState {
        pool: pool.clone(),
        config: std::sync::Arc::new(config),
        api,
    })
}
