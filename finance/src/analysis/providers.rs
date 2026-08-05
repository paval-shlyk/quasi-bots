/// Analyst price targets plus related pricing estimates / consensus.
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct PriceTargets {
    /// Consensus target price (mean of analyst targets).
    pub mean: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    /// Analysts contributing to the price target.
    pub number_of_analysts: Option<u32>,
    /// Mean recommendation score (Yahoo scale, typically 1=strong buy … 5=sell).
    pub recommendation_mean: Option<f64>,
    /// Categorical recommendation (e.g. `"buy"`, `"hold"`).
    pub recommendation_key: Option<String>,
    /// `(mean - market) / market * 100` when both are known.
    pub upside_pct: Option<f64>,

    /// Consensus EPS estimate for the current fiscal year.
    pub eps_estimate_current_year: Option<f64>,
    /// Consensus EPS estimate for next fiscal year.
    pub eps_estimate_next_year: Option<f64>,
    /// Estimated EPS growth for the current year (fraction, e.g. 0.12 = 12%).
    pub eps_growth_current_year: Option<f64>,
    /// Number of analysts on the current-year EPS estimate.
    pub eps_estimate_analysts: Option<u32>,

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
