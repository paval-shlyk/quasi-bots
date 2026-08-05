//! Portfolio holdings analysis: targets, earnings, news, technicals.

mod finnhub;
mod news_rss;
mod providers;

pub use finnhub::FinnhubProvider;
pub use news_rss::RssNewsProvider;
pub use providers::{
    AssetNewsItem, EarningsCalendarProvider, EarningsInfo,
    MockEarningsCalendarProvider, MockNewsProvider, MockPriceTargetProvider,
    NewsProvider, NullEarningsCalendarProvider, NullNewsProvider,
    NullPriceTargetProvider, PriceTargetProvider, PriceTargets,
};

use crate::indicators::{
    AnalysisConfig, TechnicalIndicators, snapshot_from_yahoo,
};
use crate::investment::{
    Asset, RestClient, fetch_holding_assets, lookup_symbol,
};

/// Holding plus optional market/context analysis.
///
/// Mark/PnL metrics live on [`Asset`]; this wrapper only adds portfolio weight
/// and external enrichment. Construct via [`OwningAssets::from_holdings`].
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct AssetWithAnalysis {
    pub asset: Asset,

    /// Share of portfolio market value (`asset.cost` / total cost × 100).
    pub weight_percentage: f64,

    pub indicators: Option<TechnicalIndicators>,
    pub targets: Option<PriceTargets>,
    pub earnings: Option<EarningsInfo>,
    pub news: Vec<AssetNewsItem>,
}

#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct OwningAssets {
    pub assets: Vec<AssetWithAnalysis>,
    //todo: add total cost
}

impl OwningAssets {
    /// Transform raw holdings into analysis rows with required portfolio weights.
    pub fn from_holdings(holdings: Vec<Asset>) -> Self {
        // `cost` is live market value of each holding.
        let total: f64 = holdings.iter().map(|a| a.cost.abs()).sum();

        let assets = holdings
            .into_iter()
            .map(|asset| {
                let weight_percentage = if total > f64::EPSILON {
                    asset.cost.abs() / total * 100.0
                } else {
                    0.0
                };
                AssetWithAnalysis {
                    asset,
                    weight_percentage,
                    indicators: None,
                    targets: None,
                    earnings: None,
                    news: Vec::new(),
                }
            })
            .collect();

        Self { assets }
    }
}

/// Services used to enrich holdings. Use mocks in tests.
pub struct AnalysisServices<T, E, N> {
    pub targets: T,
    pub earnings: E,
    pub news: N,
    pub technicals: bool,
    pub technicals_config: AnalysisConfig,
    pub news_limit: usize,
}

impl<T, E, N> AnalysisServices<T, E, N> {
    pub fn new(targets: T, earnings: E, news: N) -> Self {
        Self {
            targets,
            earnings,
            news,
            technicals: true,
            technicals_config: AnalysisConfig::default(),
            news_limit: 5,
        }
    }
}

/// Holdings only; analysis fields empty except portfolio weights.
pub async fn fetch_owning_assets(
    api: &RestClient,
) -> anyhow::Result<OwningAssets> {
    let holdings = fetch_holding_assets(api).await?;
    Ok(OwningAssets::from_holdings(holdings))
}

/// Holdings + technicals / targets / earnings / news (soft-fail per field).
pub async fn fetch_owning_assets_with_analysis<T, E, N>(
    api: &RestClient,
    services: &AnalysisServices<T, E, N>,
) -> anyhow::Result<OwningAssets>
where
    T: PriceTargetProvider,
    E: EarningsCalendarProvider,
    N: NewsProvider,
{
    let holdings = fetch_holding_assets(api).await?;
    let mut owning = OwningAssets::from_holdings(holdings);

    for row in &mut owning.assets {
        let key = lookup_symbol(&row.asset.symbol);

        if services.technicals {
            match snapshot_from_yahoo(&key, &services.technicals_config).await {
                Some(snap) => {
                    row.indicators = Some(snap);
                }
                None => {
                    tracing::debug!("no technicals for {key}");
                }
            }
        }

        match services.targets.targets(&key).await {
            Ok(t) => {
                row.targets = t.map(|mut pt| {
                    let px = row.asset.unit_market_price;
                    if let Some(mean) = pt.mean
                        && px.abs() > f64::EPSILON {
                            pt.upside_pct = Some((mean - px) / px * 100.0);
                        }
                    pt
                });
            }
            Err(e) => tracing::warn!("targets for {key}: {e}"),
        }

        match services.earnings.earnings(&key).await {
            Ok(e) => row.earnings = e,
            Err(e) => tracing::warn!("earnings for {key}: {e}"),
        }

        match services
            .news
            .recent(&key, row.asset.name.as_deref(), services.news_limit)
            .await
        {
            Ok(items) => row.news = items,
            Err(e) => tracing::warn!("news for {key}: {e}"),
        }
    }

    Ok(owning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investment::AssetEntryTrade;

    /// `cost` is live market value of the position (not entry basis).
    fn sample_asset(amount: f64, cost: f64, pl: f64, entry: f64) -> Asset {
        let unit_market_price = if amount.abs() > f64::EPSILON {
            cost / amount
        } else {
            0.0
        };
        let entry_cost = entry * amount;
        let profit_lost_percentage = if entry_cost.abs() > f64::EPSILON {
            pl / entry_cost * 100.0
        } else {
            0.0
        };
        let distance_from_entry_price = if entry.abs() > f64::EPSILON {
            (unit_market_price - entry) / entry * 100.0
        } else {
            0.0
        };
        Asset {
            name: Some("Test Co".into()),
            symbol: "TEST".into(),
            amount,
            cost,
            profit_loss: pl,
            profit_lost_percentage,
            average_entry_price: entry,
            unit_market_price,
            distance_from_entry_price,
            trades: vec![AssetEntryTrade {
                entry_price: entry,
                amount,
            }],
        }
    }

    #[test]
    fn given_holdings_when_from_holdings_then_preserves_metrics_and_sets_weights()
     {
        // Arrange: market values 1000 and 500
        let holdings = vec![
            sample_asset(10.0, 1000.0, 0.0, 100.0),
            sample_asset(5.0, 500.0, 0.0, 100.0),
        ];

        // Act
        let owning = OwningAssets::from_holdings(holdings);

        // Assert
        assert_eq!(owning.assets.len(), 2);
        assert!(
            (owning.assets[0].asset.unit_market_price - 100.0).abs() < 1e-9
        );
        assert!(
            (owning.assets[0].weight_percentage - 200.0 / 3.0).abs() < 1e-6
        );
        assert!(
            (owning.assets[1].weight_percentage - 100.0 / 3.0).abs() < 1e-6
        );
        let sum: f64 = owning.assets.iter().map(|a| a.weight_percentage).sum();
        assert!((sum - 100.0).abs() < 1e-6);
    }

    #[test]
    fn given_empty_holdings_when_from_holdings_then_weights_are_zero() {
        // Arrange / Act
        let owning = OwningAssets::from_holdings(vec![]);

        // Assert
        assert!(owning.assets.is_empty());
    }

    #[tokio::test]
    async fn given_mock_providers_when_enrich_fields_then_targets_earnings_news_present()
     {
        // Arrange
        let targets = MockPriceTargetProvider {
            mean: 150.0,
            high: 180.0,
            low: 120.0,
        };
        let earnings = MockEarningsCalendarProvider;
        let news = MockNewsProvider;

        // Act
        let t = targets.targets("TEST").await.unwrap().unwrap();
        let e = earnings.earnings("TEST").await.unwrap().unwrap();
        let n = news.recent("TEST", Some("Test"), 3).await.unwrap();

        // Assert
        assert_eq!(t.mean, Some(150.0));
        assert_eq!(t.source, "mock");
        assert!(e.next_report_at.is_some());
        assert_eq!(e.source, "mock");
        assert_eq!(n.len(), 1);
        assert!(n[0].title.contains("TEST"));
    }
}
