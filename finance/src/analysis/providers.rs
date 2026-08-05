use chrono::{Duration, Utc};

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
    ) -> impl Future<Output = anyhow::Result<Option<PriceTargets>>> + Send;
}

pub trait EarningsCalendarProvider: Send + Sync {
    fn earnings(
        &self,
        symbol: &str,
    ) -> impl Future<Output = anyhow::Result<Option<EarningsInfo>>> + Send;
}

pub trait NewsProvider: Send + Sync {
    fn recent(
        &self,
        symbol: &str,
        name: Option<&str>,
        limit: usize,
    ) -> impl Future<Output = anyhow::Result<Vec<AssetNewsItem>>> + Send;
}

#[derive(Debug, Clone)]
pub struct MockPriceTargetProvider {
    pub mean: f64,
    pub high: f64,
    pub low: f64,
}

impl PriceTargetProvider for MockPriceTargetProvider {
    async fn targets(
        &self,
        _symbol: &str,
    ) -> anyhow::Result<Option<PriceTargets>> {
        Ok(Some(PriceTargets {
            mean: Some(self.mean),
            high: Some(self.high),
            low: Some(self.low),
            upside_pct: None,
            source: "mock".into(),
        }))
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockEarningsCalendarProvider;

impl EarningsCalendarProvider for MockEarningsCalendarProvider {
    async fn earnings(
        &self,
        _symbol: &str,
    ) -> anyhow::Result<Option<EarningsInfo>> {
        let now = Utc::now();
        Ok(Some(EarningsInfo {
            next_report_at: Some(now + Duration::days(30)),
            last_report_at: Some(now - Duration::days(60)),
            eps_estimate: Some(1.25),
            eps_actual: Some(1.20),
            source: "mock".into(),
        }))
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockNewsProvider;

impl NewsProvider for MockNewsProvider {
    async fn recent(
        &self,
        symbol: &str,
        _name: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<AssetNewsItem>> {
        if limit == 0 {
            return Ok(vec![]);
        }
        Ok(vec![AssetNewsItem {
            title: format!("Mock headline for {symbol}"),
            published_at: Some(Utc::now()),
            url: Some(format!("https://example.com/news/{symbol}")),
            summary: Some("Mock summary".into()),
            source: Some("mock".into()),
        }])
    }
}

/// No-op providers (skip network).
#[derive(Debug, Clone, Default)]
pub struct NullPriceTargetProvider;

impl PriceTargetProvider for NullPriceTargetProvider {
    async fn targets(
        &self,
        _symbol: &str,
    ) -> anyhow::Result<Option<PriceTargets>> {
        Ok(None)
    }
}

#[derive(Debug, Clone, Default)]
pub struct NullEarningsCalendarProvider;

impl EarningsCalendarProvider for NullEarningsCalendarProvider {
    async fn earnings(
        &self,
        _symbol: &str,
    ) -> anyhow::Result<Option<EarningsInfo>> {
        Ok(None)
    }
}

#[derive(Debug, Clone, Default)]
pub struct NullNewsProvider;

impl NewsProvider for NullNewsProvider {
    async fn recent(
        &self,
        _symbol: &str,
        _name: Option<&str>,
        _limit: usize,
    ) -> anyhow::Result<Vec<AssetNewsItem>> {
        Ok(vec![])
    }
}
