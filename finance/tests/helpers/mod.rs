//! Shared test helpers for finance integration tests.

use chrono::{Duration, Utc};
use finance::analysis::{
    AssetNewsItem, EarningsCalendarProvider, EarningsInfo, NewsProvider,
    PriceTargetProvider, PriceTargets,
};

#[derive(Debug, Clone)]
pub struct MockPriceTargetProvider {
    pub mean: f64,
    pub high: f64,
    pub low: f64,
}

impl PriceTargetProvider for MockPriceTargetProvider {
    async fn targets(&self, _symbol: &str) -> anyhow::Result<PriceTargets> {
        Ok(PriceTargets {
            mean: Some(self.mean),
            high: Some(self.high),
            low: Some(self.low),
            number_of_analysts: Some(10),
            recommendation_mean: Some(2.0),
            recommendation_key: Some("buy".into()),
            upside_pct: None,
            eps_estimate_current_year: Some(5.0),
            eps_estimate_next_year: Some(5.5),
            eps_growth_current_year: Some(0.1),
            eps_estimate_analysts: Some(12),
            source: "mock".into(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockEarningsCalendarProvider;

impl EarningsCalendarProvider for MockEarningsCalendarProvider {
    async fn earnings(&self, _symbol: &str) -> anyhow::Result<EarningsInfo> {
        let now = Utc::now();
        Ok(EarningsInfo {
            next_report_at: Some(now + Duration::days(30)),
            last_report_at: Some(now - Duration::days(60)),
            eps_estimate: Some(1.25),
            eps_actual: Some(1.20),
            source: "mock".into(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MockNewsProvider {
    pub limit: usize,
}

impl Default for MockNewsProvider {
    fn default() -> Self {
        Self { limit: 5 }
    }
}

impl NewsProvider for MockNewsProvider {
    async fn recent(
        &self,
        symbol: &str,
        _name: Option<&str>,
    ) -> anyhow::Result<Vec<AssetNewsItem>> {
        if self.limit == 0 {
            return Ok(vec![]);
        }
        Ok(vec![AssetNewsItem {
            title: format!("Mock headline for {symbol}"),
            published_at: Some(Utc::now()),
            url: Some(format!("https://example.com/news/{symbol}")),
            summary: Some("Mock summary".into()),
            source: Some("mock".into()),
        }]
        .into_iter()
        .take(self.limit)
        .collect())
    }
}
