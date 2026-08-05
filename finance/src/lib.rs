pub mod analysis;
pub mod expenses;
pub mod indicators;
pub mod investment;
pub mod model;
mod recommendations;
mod state;

pub use analysis::{
    AnalysisServices, AssetWithAnalysis, OwningAssets, fetch_owning_assets,
    fetch_owning_assets_with_analysis,
};
pub use state::FinanceState;

pub use investment::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    //dummy rss source
    pub rss_source: reqwest::Url,

    pub provider: investment::ProviderConfig,
}

pub async fn connect(
    config: Config,
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<FinanceState> {
    let api = investment::RestClient::new(
        config.provider.base_url.clone(),
        config.provider.api_key.clone(),
        config.provider.api_secret.clone(),
    );

    expenses::init_predefined(pool).await?;

    Ok(FinanceState {
        pool: pool.clone(),
        config: std::sync::Arc::new(config),
        api,
    })
}
