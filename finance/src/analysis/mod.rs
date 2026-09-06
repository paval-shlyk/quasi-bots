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
    Asset, AssetClass, RestClient, assemble_holdings, load_dzengi_snapshot,
    lookup_symbol, nav_from_wallet, usd_wallet,
};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PositionInclude {
    Trades,
    Indicators,
    Earnings,
    Targets,
    News,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub struct PositionQuery {
    #[serde(default)]
    pub include: Vec<PositionInclude>,
    #[serde(default)]
    pub symbols: Option<Vec<String>>,
}

impl PositionQuery {
    pub fn wants(&self, block: PositionInclude) -> bool {
        self.include.contains(&block)
    }
}

/// Holding plus optional market/context analysis.
///
/// Mark/PnL metrics live on [`Asset`]; this wrapper only adds portfolio weight
/// and external enrichment. Construct via [`OwningAssets::from_holdings`].
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct AssetWithAnalysis {
    #[serde(flatten)]
    pub asset: Asset,

    /// Percent of NAV including cash (`market_value / nav × 100`).
    pub weight_percentage: f64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicators: Option<TechnicalIndicators>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<PriceTargets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earnings: Option<EarningsInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    ///
    /// `nav` is the wallet Equity (cash + reserved). When absent, weights fall
    /// back to share of position market value and will not sum to 100 if cash > 0.
    pub fn from_holdings(holdings: Vec<Asset>, nav: Option<f64>) -> Self {
        let positions_value: f64 =
            holdings.iter().map(|a| a.market_value.abs()).sum();
        let denom =
            nav.filter(|n| *n > f64::EPSILON).unwrap_or(positions_value);

        let assets = holdings
            .into_iter()
            .map(|asset| {
                let weight_percentage = if denom > f64::EPSILON {
                    asset.market_value.abs() / denom * 100.0
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

fn same_order_of_magnitude(a: f64, b: f64) -> bool {
    if a.abs() <= f64::EPSILON || b.abs() <= f64::EPSILON {
        return false;
    }
    let ratio = (a / b).abs();
    (0.1..=10.0).contains(&ratio)
}

fn should_attach_targets(class: AssetClass, requested: bool) -> bool {
    requested && class == AssetClass::Equity
}

fn apply_query(mut holdings: Vec<Asset>, query: &PositionQuery) -> Vec<Asset> {
    if let Some(symbols) = &query.symbols {
        holdings.retain(|a| {
            symbols.iter().any(|s| s.eq_ignore_ascii_case(&a.symbol))
        });
    }
    if !query.wants(PositionInclude::Trades) {
        for a in &mut holdings {
            a.trades.clear();
        }
    }
    holdings
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

async fn load_holdings_and_nav(
    api: &RestClient,
) -> anyhow::Result<(Vec<Asset>, Option<f64>)> {
    let snapshot = load_dzengi_snapshot(api).await?;
    let holdings = assemble_holdings(api, &snapshot).await?;
    let (cash, reserved) = usd_wallet(&snapshot.account);
    Ok((holdings.assets, nav_from_wallet(cash, reserved)))
}

/// Holdings only; analysis fields empty except portfolio weights.
pub async fn fetch_owning_assets(
    api: &RestClient,
    query: &PositionQuery,
) -> anyhow::Result<OwningAssets> {
    let (holdings, nav) = load_holdings_and_nav(api).await?;
    Ok(OwningAssets::from_holdings(
        apply_query(holdings, query),
        nav,
    ))
}

/// Holdings + technicals / targets / earnings / news.
///
/// Missing providers leave fields empty. Per-symbol enrichment errors are
/// soft-failed (warn + skip field) so one unknown ticker (e.g. index CFD
/// `US500`) does not abort the whole portfolio. Blocks are fetched only when
/// `query.include` asks for them.
pub async fn fetch_owning_assets_with_analysis<T, E, N>(
    api: &RestClient,
    services: &AnalysisServices<T, E, N>,
    query: &PositionQuery,
) -> anyhow::Result<OwningAssets>
where
    T: PriceTargetProvider,
    E: EarningsCalendarProvider,
    N: NewsProvider,
{
    let (holdings, nav) = load_holdings_and_nav(api).await?;
    let mut owning =
        OwningAssets::from_holdings(apply_query(holdings, query), nav);

    let want_indicators =
        query.wants(PositionInclude::Indicators) && services.technicals;
    let want_targets = query.wants(PositionInclude::Targets);
    let want_earnings = query.wants(PositionInclude::Earnings);
    let want_news = query.wants(PositionInclude::News);

    if !want_indicators && !want_targets && !want_earnings && !want_news {
        return Ok(owning);
    }

    for row in &mut owning.assets {
        let key = lookup_symbol(&row.asset.symbol);

        if want_indicators {
            match snapshot_from_yahoo(&key, &services.technicals_config).await {
                Some(mut snap) => {
                    if same_order_of_magnitude(
                        snap.price,
                        row.asset.unit_market_price,
                    ) {
                        snap.price = row.asset.unit_market_price;
                        row.indicators = Some(snap);
                    } else {
                        tracing::warn!(
                            symbol = %row.asset.symbol,
                            yahoo = snap.price,
                            mark = row.asset.unit_market_price,
                            "dropping indicators: Yahoo price is a different instrument"
                        );
                    }
                }
                None => {
                    tracing::debug!("no technicals for {key}");
                }
            }
        }

        if should_attach_targets(row.asset.asset_class, want_targets)
            && let Some(provider) = &services.targets
        {
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

        if want_earnings && let Some(provider) = &services.earnings {
            match provider.earnings(&key).await {
                Ok(info) => row.earnings = Some(info),
                Err(e) => tracing::warn!("earnings for {key}: {e}"),
            }
        }

        if want_news && let Some(provider) = &services.news {
            match provider
                .recent(&row.asset.symbol, row.asset.name.as_deref())
                .await
            {
                Ok(items) => row.news = items.into_iter().take(3).collect(),
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

    /// `market_value` is live market value of the position (not entry basis).
    fn sample_asset(
        amount: f64,
        market_value: f64,
        pl: f64,
        entry: f64,
    ) -> Asset {
        let unit_market_price = if amount.abs() > f64::EPSILON {
            market_value / amount
        } else {
            0.0
        };
        let entry_cost = entry * amount;
        let unrealized_pnl_pct = if entry_cost.abs() > f64::EPSILON {
            pl / entry_cost * 100.0
        } else {
            0.0
        };
        Asset {
            name: Some("Test Co".into()),
            symbol: "TEST".into(),
            asset_class: AssetClass::Equity,
            leverage: false,
            amount,
            average_entry_price: entry,
            cost_basis: entry_cost,
            unit_market_price,
            market_value,
            unrealized_pnl: pl,
            unrealized_pnl_pct,
            currency: "USD".into(),
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
        let owning = OwningAssets::from_holdings(holdings, None);

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
        let owning = OwningAssets::from_holdings(vec![], None);

        // Assert
        assert!(owning.assets.is_empty());
    }

    #[test]
    fn given_cash_in_nav_when_from_holdings_then_named_weights_fall() {
        let holdings = vec![sample_asset(1.0, 80.0, 0.0, 80.0)];
        let owning = OwningAssets::from_holdings(holdings, Some(100.0));
        assert!((owning.assets[0].weight_percentage - 80.0).abs() < 1e-9);
    }

    #[test]
    fn given_default_include_when_serialized_then_is_flat_and_lean() {
        let holdings = vec![sample_asset(1.0, 80.0, 0.0, 80.0)];
        let owning = OwningAssets::from_holdings(
            apply_query(holdings, &PositionQuery::default()),
            Some(100.0),
        );
        let v = serde_json::to_value(&owning).unwrap();
        let asset = &v["assets"][0];
        assert!(asset.get("symbol").is_some());
        assert!(asset.get("asset").is_none());
        assert!(asset.get("market_value").is_some());
        assert!(asset.get("cost").is_none());
        assert!(asset.get("profit_loss").is_none());
        assert!(asset.get("trades").is_none());
        assert!(asset.get("news").is_none());
        assert!(asset.get("indicators").is_none());
        assert!(asset.get("targets").is_none());
        assert!(asset.get("earnings").is_none());
    }

    #[test]
    fn given_yahoo_price_wrong_instrument_when_magnitude_check_then_rejects() {
        assert!(!same_order_of_magnitude(43.16, 4278.56));
        assert!(same_order_of_magnitude(7568.1, 7631.47));
    }

    #[test]
    fn given_targets_include_when_class_is_not_equity_then_skipped() {
        assert!(!should_attach_targets(AssetClass::Commodity, true));
        assert!(!should_attach_targets(AssetClass::Index, true));
        assert!(!should_attach_targets(AssetClass::Crypto, true));
        assert!(should_attach_targets(AssetClass::Equity, true));
        assert!(!should_attach_targets(AssetClass::Equity, false));
    }

    #[test]
    fn given_symbols_filter_when_apply_query_then_keeps_matching_and_strips_trades()
     {
        let mut tsla = sample_asset(1.0, 80.0, 0.0, 80.0);
        tsla.symbol = "TSLA".into();
        let mut gold = sample_asset(1.0, 20.0, 0.0, 20.0);
        gold.symbol = "Gold".into();
        gold.asset_class = AssetClass::Commodity;
        let query = PositionQuery {
            include: vec![],
            symbols: Some(vec!["TSLA".into()]),
        };
        let out = apply_query(vec![tsla, gold], &query);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].symbol, "TSLA");
        assert!(out[0].trades.is_empty());
    }
}
