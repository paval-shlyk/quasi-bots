#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct PriceTargets {
    pub mean: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub upside_pct: Option<f64>,
    pub source: String,
}

#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct EarningsInfo {
    pub next_report_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_report_at: Option<chrono::DateTime<chrono::Utc>>,
    pub eps_estimate: Option<f64>,
    pub eps_actual: Option<f64>,
    pub source: String,
}

#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct AssetNewsItem {
    pub title: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub url: Option<String>,
    pub summary: Option<String>,
    pub source: Option<String>,
}

pub trait PriceTargetProvider: Send + Sync {
    fn targets(
        &self,
        symbol: &str,
    ) -> impl Future<Output = anyhow::Result<PriceTargets>> + Send;
}

pub trait EarningsCalendarProvider: Send + Sync {
    fn earnings(
        &self,
        symbol: &str,
    ) -> impl Future<Output = anyhow::Result<EarningsInfo>> + Send;
}

pub trait NewsProvider: Send + Sync {
    /// Recent headlines; limit is owned by the provider implementation.
    fn recent(
        &self,
        symbol: &str,
        name: Option<&str>,
    ) -> impl Future<Output = anyhow::Result<Vec<AssetNewsItem>>> + Send;
}
