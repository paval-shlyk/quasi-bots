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
    Asset, AssetClass, RestClient, assemble_holdings, asset_class_fallback,
    load_dzengi_snapshot, lookup_symbol, nav_from_wallet, usd_wallet,
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
pub enum AnalysisInclude {
    Indicators,
    Earnings,
    Targets,
    News,
}

impl AnalysisInclude {
    pub fn all() -> Vec<Self> {
        vec![Self::News, Self::Targets, Self::Earnings, Self::Indicators]
    }
}

fn wants_block(include: &[AnalysisInclude], block: AnalysisInclude) -> bool {
    include.is_empty() || include.contains(&block)
}

fn require_symbols(symbols: &[String]) -> anyhow::Result<()> {
    if symbols.is_empty() {
        anyhow::bail!("trading_analysis requires at least one symbol");
    }
    Ok(())
}

/// Holding plus portfolio weight. Construct via [`OwningAssets::from_holdings`].
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct AssetWithWeight {
    #[serde(flatten)]
    pub asset: Asset,

    /// Percent of NAV including cash (`market_value / nav × 100`).
    pub weight_percentage: f64,
}

/// Research extras for one symbol. Not a position.
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct SymbolAnalysis {
    pub symbol: String,
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
pub struct AssetAnalysis {
    pub symbols: Vec<SymbolAnalysis>,
}

#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct OwningAssets {
    pub assets: Vec<AssetWithWeight>,
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
                AssetWithWeight {
                    asset,
                    weight_percentage,
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

fn filter_symbols(
    mut holdings: Vec<Asset>,
    symbols: Option<&[String]>,
) -> Vec<Asset> {
    if let Some(symbols) = symbols {
        holdings.retain(|a| {
            symbols.iter().any(|s| s.eq_ignore_ascii_case(&a.symbol))
        });
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

/// Dzengi book only (derived weights, lots). No research extras.
pub async fn fetch_owning_assets(
    api: &RestClient,
    symbols: Option<&[String]>,
) -> anyhow::Result<OwningAssets> {
    let (holdings, nav) = load_holdings_and_nav(api).await?;
    Ok(OwningAssets::from_holdings(
        filter_symbols(holdings, symbols),
        nav,
    ))
}

/// News / targets / earnings / indicators for named symbols. Not a position book.
///
/// `symbols` must be non-empty. Empty `include` means all four blocks.
/// Per-symbol enrichment errors are soft-failed (warn + skip field).
pub async fn fetch_asset_analysis<T, E, N>(
    api: &RestClient,
    services: &AnalysisServices<T, E, N>,
    symbols: &[String],
    include: &[AnalysisInclude],
) -> anyhow::Result<AssetAnalysis>
where
    T: PriceTargetProvider,
    E: EarningsCalendarProvider,
    N: NewsProvider,
{
    require_symbols(symbols)?;

    let (holdings, _) = load_holdings_and_nav(api).await?;
    let want_indicators = wants_block(include, AnalysisInclude::Indicators)
        && services.technicals;
    let want_targets = wants_block(include, AnalysisInclude::Targets);
    let want_earnings = wants_block(include, AnalysisInclude::Earnings);
    let want_news = wants_block(include, AnalysisInclude::News);

    let mut rows = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        let holding = holdings
            .iter()
            .find(|a| a.symbol.eq_ignore_ascii_case(symbol));
        let class = holding
            .map(|h| h.asset_class)
            .unwrap_or_else(|| asset_class_fallback(symbol));
        let name = holding.and_then(|h| h.name.as_deref());
        let mark = holding.map(|h| h.unit_market_price);
        let key = lookup_symbol(symbol);

        let mut row = SymbolAnalysis {
            symbol: holding
                .map(|h| h.symbol.clone())
                .unwrap_or_else(|| symbol.clone()),
            indicators: None,
            targets: None,
            earnings: None,
            news: Vec::new(),
        };

        if want_indicators {
            match snapshot_from_yahoo(&key, &services.technicals_config).await {
                Some(mut snap) => {
                    let keep = match mark {
                        Some(px) if same_order_of_magnitude(snap.price, px) => {
                            snap.price = px;
                            true
                        }
                        Some(px) => {
                            tracing::warn!(
                                symbol = %row.symbol,
                                yahoo = snap.price,
                                mark = px,
                                "dropping indicators: Yahoo price is a different instrument"
                            );
                            false
                        }
                        None => true,
                    };
                    if keep {
                        row.indicators = Some(snap);
                    }
                }
                None => {
                    tracing::debug!("no technicals for {key}");
                }
            }
        }

        if should_attach_targets(class, want_targets)
            && let Some(provider) = &services.targets
        {
            match provider.targets(&key).await {
                Ok(mut pt) => {
                    if let (Some(mean), Some(px)) = (pt.mean, mark)
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
            match provider.recent(symbol, name).await {
                Ok(items) => row.news = items.into_iter().take(3).collect(),
                Err(e) => tracing::warn!("news for {key}: {e}"),
            }
        }

        rows.push(row);
    }

    Ok(AssetAnalysis { symbols: rows })
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
    fn given_book_when_serialized_then_is_flat_with_lots_and_no_research() {
        let holdings = vec![sample_asset(1.0, 80.0, 0.0, 80.0)];
        let owning = OwningAssets::from_holdings(holdings, Some(100.0));
        let v = serde_json::to_value(&owning).unwrap();
        let asset = &v["assets"][0];
        assert!(asset.get("symbol").is_some());
        assert!(asset.get("asset").is_none());
        assert!(asset.get("market_value").is_some());
        assert!(asset.get("weight_percentage").is_some());
        assert!(asset.get("trades").is_some());
        assert!(asset.get("cost").is_none());
        assert!(asset.get("news").is_none());
        assert!(asset.get("indicators").is_none());
        assert!(asset.get("targets").is_none());
        assert!(asset.get("earnings").is_none());
    }

    #[test]
    fn given_analysis_row_when_serialized_then_has_no_book_fields() {
        let row = SymbolAnalysis {
            symbol: "TSLA".into(),
            indicators: None,
            targets: None,
            earnings: None,
            news: vec![AssetNewsItem {
                title: "Tesla headline".into(),
                published_at: None,
                url: Some("https://example.com".into()),
                summary: None,
                source: Some("markets".into()),
            }],
        };
        let v = serde_json::to_value(&AssetAnalysis { symbols: vec![row] })
            .unwrap();
        let s = &v["symbols"][0];
        assert_eq!(s["symbol"], "TSLA");
        assert!(s.get("news").is_some());
        assert!(s.get("amount").is_none());
        assert!(s.get("market_value").is_none());
        assert!(s.get("weight_percentage").is_none());
        assert!(s.get("indicators").is_none());
        assert!(s["news"][0].get("summary").is_none());
    }

    #[test]
    fn given_empty_symbols_when_require_symbols_then_errors() {
        let err = require_symbols(&[]).unwrap_err();
        assert!(err.to_string().contains("at least one symbol"));
    }

    #[test]
    fn given_empty_symbols_when_wants_block_then_all_blocks_are_on() {
        assert!(wants_block(&[], AnalysisInclude::News));
        assert!(wants_block(&[AnalysisInclude::News], AnalysisInclude::News));
        assert!(!wants_block(
            &[AnalysisInclude::News],
            AnalysisInclude::Targets
        ));
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
    fn given_symbols_filter_when_filter_symbols_then_keeps_matching_lots() {
        let mut tsla = sample_asset(1.0, 80.0, 0.0, 80.0);
        tsla.symbol = "TSLA".into();
        let mut gold = sample_asset(1.0, 20.0, 0.0, 20.0);
        gold.symbol = "Gold".into();
        gold.asset_class = AssetClass::Commodity;
        let symbols = ["TSLA".to_string()];
        let out = filter_symbols(vec![tsla, gold], Some(&symbols));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].symbol, "TSLA");
        assert_eq!(out[0].trades.len(), 1);
    }
}
