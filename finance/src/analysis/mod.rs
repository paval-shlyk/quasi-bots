//! Portfolio holdings analysis: targets, earnings, news, technicals.

mod finnhub;
mod news_rss;
mod providers;
mod yahoo_targets;

pub use finnhub::FinnhubProvider;
pub use news_rss::RssNewsProvider;
pub use providers::{
    AssetNewsItem, EarningsCalendarProvider, EarningsInfo, NewsProvider,
    PriceTargetProvider, PriceTargets,
};
pub use yahoo_targets::YahooPriceTargetProvider;

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

/// Services used to enrich holdings. Absent providers skip that field.
pub struct AnalysisServices<T, E, N> {
    pub targets: Option<T>,
    pub earnings: Option<E>,
    pub news: Option<N>,
    pub technicals: bool,
    pub technicals_config: AnalysisConfig,
}

impl<T, E, N> AnalysisServices<T, E, N> {
    pub fn new(
        targets: Option<T>,
        earnings: Option<E>,
        news: Option<N>,
    ) -> Self {
        Self {
            targets,
            earnings,
            news,
            technicals: true,
            technicals_config: AnalysisConfig::default(),
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

/// Holdings + technicals / targets / earnings / news.
///
/// Missing providers leave fields empty. Per-symbol enrichment errors are
/// soft-failed (warn + skip field) so one unknown ticker (e.g. index CFD
/// `US500`) does not abort the whole portfolio.
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

        if let Some(provider) = &services.targets {
            match provider.targets(&key).await {
                Ok(mut pt) => {
                    let px = row.asset.unit_market_price;
                    if let Some(mean) = pt.mean
                        && px.abs() > f64::EPSILON
                    {
                        pt.upside_pct = Some((mean - px) / px * 100.0);
                    }
                    row.targets = Some(pt);
                }
                Err(e) => {
                    tracing::warn!("targets for {key}: {e}");
                }
            }
        }

        if let Some(provider) = &services.earnings {
            match provider.earnings(&key).await {
                Ok(info) => row.earnings = Some(info),
                Err(e) => tracing::warn!("earnings for {key}: {e}"),
            }
        }

        if let Some(provider) = &services.news {
            match provider.recent(&key, row.asset.name.as_deref()).await {
                Ok(items) => row.news = items,
                Err(e) => tracing::warn!("news for {key}: {e}"),
            }
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
        let profit_lost_pct = if entry_cost.abs() > f64::EPSILON {
            pl / entry_cost * 100.0
        } else {
            0.0
        };
        let distance_from_entry_price_pct = if entry.abs() > f64::EPSILON {
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
            profit_lost_pct,
            average_entry_price: entry,
            unit_market_price,
            distance_from_entry_price_pct,
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
}
