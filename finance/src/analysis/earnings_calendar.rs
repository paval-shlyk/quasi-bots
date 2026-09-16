//! First-class earnings / report-date calendar (P1-B1).
//!
//! "Revenue" here means **company earnings report publish dates**, not P&L.

use chrono::{DateTime, Duration, Utc};

use super::providers::{EarningsCalendarProvider, EarningsInfo};
use crate::investment::{
    Asset, AssetClass, RestClient, assemble_holdings, load_dzengi_snapshot,
    lookup_symbol,
};

/// Default upcoming window (days) for book scan.
pub const DEFAULT_HORIZON_DAYS: i64 = 14;
/// Default lookback (days) for recently reported names.
pub const DEFAULT_INCLUDE_PAST_DAYS: i64 = 1;

/// Query for [`fetch_earnings_calendar`].
#[derive(Debug, Clone)]
pub struct EarningsCalendarQuery {
    /// Optional symbol filter. `None` / empty = all **equity** names in the open book.
    pub symbols: Option<Vec<String>>,
    /// Upcoming reports within this many days (default [`DEFAULT_HORIZON_DAYS`]).
    pub horizon_days: i64,
    /// Recently reported within this many days (default [`DEFAULT_INCLUDE_PAST_DAYS`]).
    pub include_past_days: i64,
}

impl Default for EarningsCalendarQuery {
    fn default() -> Self {
        Self {
            symbols: None,
            horizon_days: DEFAULT_HORIZON_DAYS,
            include_past_days: DEFAULT_INCLUDE_PAST_DAYS,
        }
    }
}

/// One calendar row. Soft-fail via `error`; dates / estimates may be null.
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct EarningsCalendarEvent {
    pub symbol: String,
    pub name: Option<String>,
    pub next_report_at: Option<DateTime<Utc>>,
    pub last_report_at: Option<DateTime<Utc>>,
    /// Fiscal period when known (e.g. `"2026-Q3"`).
    pub period: Option<String>,
    pub eps_estimate: Option<f64>,
    pub eps_actual: Option<f64>,
    pub source: Option<String>,
    pub error: Option<String>,
}

/// Batch response for MCP `trading_earnings_calendar`.
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct EarningsCalendarResponse {
    pub as_of: DateTime<Utc>,
    pub events: Vec<EarningsCalendarEvent>,
}

impl EarningsCalendarEvent {
    fn from_info(
        symbol: String,
        name: Option<String>,
        info: EarningsInfo,
    ) -> Self {
        Self {
            symbol,
            name,
            next_report_at: info.next_report_at,
            last_report_at: info.last_report_at,
            period: info.period,
            eps_estimate: info.eps_estimate,
            eps_actual: info.eps_actual,
            source: Some(info.source),
            error: None,
        }
    }

    fn err(
        symbol: String,
        name: Option<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            symbol,
            name,
            next_report_at: None,
            last_report_at: None,
            period: None,
            eps_estimate: None,
            eps_actual: None,
            source: None,
            error: Some(error.into()),
        }
    }
}

/// Match broker symbols that differ by trailing `.` or `/USD_LEVERAGE`.
fn symbols_match(requested: &str, holding: &str) -> bool {
    if requested.eq_ignore_ascii_case(holding) {
        return true;
    }
    let req = lookup_symbol(requested);
    let hold = lookup_symbol(holding);
    req.eq_ignore_ascii_case(&hold)
}

/// Map broker symbol → Finnhub earnings ticker (same base map as TA).
///
/// Rejects Yahoo-only index / futures / crypto keys that Finnhub will not
/// resolve for an earnings calendar (wrong-instrument / non-equity).
pub fn finnhub_earnings_symbol(broker_symbol: &str) -> Result<String, String> {
    let key = lookup_symbol(broker_symbol);
    if key.starts_with('^')
        || key.contains('=')
        || (key.contains('-') && key.chars().any(|c| c.is_ascii_digit()))
    {
        return Err(format!(
            "finnhub earnings map for {broker_symbol}: key {key} is not a Finnhub equity ticker (wrong instrument or non-equity)"
        ));
    }
    if key.is_empty() {
        return Err(format!(
            "finnhub earnings map for {broker_symbol}: empty lookup key"
        ));
    }
    Ok(key)
}

fn in_window(
    info: &EarningsInfo,
    query: &EarningsCalendarQuery,
    as_of: DateTime<Utc>,
) -> bool {
    let horizon = Duration::days(query.horizon_days.max(0));
    let past = Duration::days(query.include_past_days.max(0));
    let upcoming = info.next_report_at.is_some_and(|t| {
        t >= as_of - Duration::hours(12) && t <= as_of + horizon
    });
    let recent = info
        .last_report_at
        .is_some_and(|t| t <= as_of && t >= as_of - past);
    upcoming || recent
}

struct Target {
    symbol: String,
    name: Option<String>,
    /// When true, always emit a row (explicit symbol filter).
    force: bool,
}

fn equity_book_targets(holdings: &[Asset]) -> Vec<Target> {
    holdings
        .iter()
        .filter(|h| h.asset_class == AssetClass::Equity)
        .map(|h| Target {
            symbol: h.symbol.clone(),
            name: h.name.clone(),
            force: false,
        })
        .collect()
}

fn explicit_targets(holdings: &[Asset], symbols: &[String]) -> Vec<Target> {
    symbols
        .iter()
        .map(|s| {
            let holding = holdings.iter().find(|h| symbols_match(s, &h.symbol));
            Target {
                symbol: holding
                    .map(|h| h.symbol.clone())
                    .unwrap_or_else(|| s.clone()),
                name: holding.and_then(|h| h.name.clone()),
                force: true,
            }
        })
        .collect()
}

/// Held-book earnings / report dates via Finnhub (or any [`EarningsCalendarProvider`]).
///
/// - Default (no / empty `symbols`): open **equity** names only; skip Gold/US500/TON.
/// - Optional `symbols`: include those names (any class); soft per-symbol `error`.
/// - Window: `horizon_days` upcoming + `include_past_days` recently reported.
/// - Book scan omits names with no dates in-window; explicit filter always returns a row.
/// - No Telegram, news, or TA.
pub async fn fetch_earnings_calendar<E: EarningsCalendarProvider>(
    api: &RestClient,
    provider: &E,
    query: &EarningsCalendarQuery,
) -> anyhow::Result<EarningsCalendarResponse> {
    let snapshot = load_dzengi_snapshot(api).await?;
    let holdings = assemble_holdings(api, &snapshot).await?;
    let as_of = Utc::now();

    let explicit = query
        .symbols
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    let targets = if explicit {
        explicit_targets(&holdings.assets, query.symbols.as_ref().unwrap())
    } else {
        equity_book_targets(&holdings.assets)
    };

    let mut events = Vec::with_capacity(targets.len());
    for target in targets {
        let key = match finnhub_earnings_symbol(&target.symbol) {
            Ok(k) => k,
            Err(msg) => {
                if target.force {
                    events.push(EarningsCalendarEvent::err(
                        target.symbol,
                        target.name,
                        msg,
                    ));
                }
                // Book scan: skip non-equities / unmappable (already filtered to equity,
                // but Yahoo-style leftovers still soft-skip).
                continue;
            }
        };

        match provider.earnings(&key).await {
            Ok(info) => {
                // Explicit filter: always emit a row (data or nulls).
                // Book scan: only names with dates inside the window.
                let emit = if target.force {
                    true
                } else {
                    (info.next_report_at.is_some()
                        || info.last_report_at.is_some())
                        && in_window(&info, query, as_of)
                };
                if emit {
                    events.push(EarningsCalendarEvent::from_info(
                        target.symbol,
                        target.name,
                        info,
                    ));
                }
            }
            Err(e) => {
                let msg = format!("earnings for {key}: {e}");
                tracing::warn!("{msg}");
                if target.force {
                    events.push(EarningsCalendarEvent::err(
                        target.symbol,
                        target.name,
                        msg,
                    ));
                }
            }
        }
    }

    Ok(EarningsCalendarResponse { as_of, events })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investment::{AssetEntryTrade, MarginMode};

    fn sample_equity(symbol: &str) -> Asset {
        Asset {
            name: Some(format!("{symbol} Corp")),
            symbol: symbol.into(),
            asset_class: AssetClass::Equity,
            margin_mode: MarginMode::Collateral,
            leverage: false,
            amount: 1.0,
            average_entry_price: 100.0,
            cost_basis: 100.0,
            unit_market_price: 110.0,
            market_value: 110.0,
            unrealized_pnl: 10.0,
            unrealized_pnl_pct: 10.0,
            currency: "USD".into(),
            trades: vec![AssetEntryTrade {
                entry_price: 100.0,
                amount: 1.0,
            }],
        }
    }

    #[test]
    fn given_gold_us500_ton_when_finnhub_earnings_symbol_then_rejects() {
        for s in ["Gold", "US500", "TON", "TON/USD_LEVERAGE"] {
            let err = finnhub_earnings_symbol(s).unwrap_err();
            assert!(
                err.contains("not a Finnhub equity ticker"),
                "symbol {s}: {err}"
            );
        }
    }

    #[test]
    fn given_equity_broker_symbol_when_finnhub_earnings_symbol_then_bare_ticker()
     {
        assert_eq!(finnhub_earnings_symbol("TSLA").unwrap(), "TSLA");
        assert_eq!(finnhub_earnings_symbol("TSM.").unwrap(), "TSM");
        assert_eq!(finnhub_earnings_symbol("IBM/USD_LEVERAGE").unwrap(), "IBM");
    }

    #[test]
    fn given_window_when_in_window_then_upcoming_and_recent() {
        let as_of = Utc::now();
        let query = EarningsCalendarQuery::default();
        let upcoming = EarningsInfo {
            next_report_at: Some(as_of + Duration::days(7)),
            last_report_at: None,
            period: Some("2026-Q3".into()),
            eps_estimate: Some(1.0),
            eps_actual: None,
            source: "test".into(),
        };
        assert!(in_window(&upcoming, &query, as_of));

        let far = EarningsInfo {
            next_report_at: Some(as_of + Duration::days(60)),
            last_report_at: Some(as_of - Duration::days(90)),
            period: None,
            eps_estimate: None,
            eps_actual: None,
            source: "test".into(),
        };
        assert!(!in_window(&far, &query, as_of));

        let recent = EarningsInfo {
            next_report_at: None,
            last_report_at: Some(as_of - Duration::hours(6)),
            period: Some("2026-Q2".into()),
            eps_estimate: None,
            eps_actual: Some(0.9),
            source: "test".into(),
        };
        assert!(in_window(&recent, &query, as_of));
    }

    #[test]
    fn given_mixed_book_when_equity_book_targets_then_skips_non_equity() {
        let mut gold = sample_equity("Gold");
        gold.asset_class = AssetClass::Commodity;
        gold.symbol = "Gold".into();
        let mut us500 = sample_equity("US500");
        us500.asset_class = AssetClass::Index;
        us500.symbol = "US500".into();
        let holdings = vec![sample_equity("IBM"), gold, us500];
        let t = equity_book_targets(&holdings);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].symbol, "IBM");
    }
}
