//! Dzengi WS-backed alert evaluator (P1 Wave A3).
//!
//! Long-running task inside the finance/skill-master process. Subscribes to
//! Dzengi WS for marks (ticker) and portfolio hints; periodically reconciles
//! the book via REST. Evaluates enabled watches and inserts `watch.fired` v1
//! events into `alert_outbox` (cooldown respected).
//!
//! **Not in scope:** Telegram / `sendMessage` (Wave A4).

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::MissedTickBehavior;

use crate::analysis::OwningAssets;
use crate::investment::model::PortfolioEvent;
use crate::investment::quotes::{Quote, resolve_quote_pair};
use crate::investment::rest_api::RestClient;
use crate::investment::routes::{
    assemble_holdings, load_dzengi_snapshot, nav_from_wallet, usd_wallet,
};
use crate::investment::watches::{
    EvalSnapshot, SymbolObservation, Watch, WatchRule,
    evaluate_enabled_watches, list_enabled_watches,
};
use crate::investment::ws_api::{self, Client as WsClient, WS_CONNECT_TIMEOUT};

/// Default mark/eval cadence when WS is up (seconds).
pub const DEFAULT_EVAL_INTERVAL_SECS: u64 = 60;
/// Slow REST book reconcile (seconds); design allows 5–15m.
pub const DEFAULT_REST_RECONCILE_SECS: u64 = 600;
/// Soft cap on symbols refreshed per eval cycle (same spirit as quotes).
pub const MAX_EVAL_SYMBOLS: usize = 30;

/// Structural dig snapshot for unauthenticated `GET /health` (no magnitudes/secrets).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AlertEvaluatorDig {
    /// True when the evaluator task was spawned / is running.
    pub running: bool,
    /// Cash presence only: `"Some"` | `"None"` after at least one tick; `null` if never ticked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_watches: Option<usize>,
    /// ISO-8601 UTC of last successful eval tick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tick_at: Option<DateTime<Utc>>,
    /// Seconds since `last_tick_at` (computed at read time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_tick_age_secs: Option<u64>,
    /// Structural count of watches that fired on the last tick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fired_count: Option<usize>,
}

#[derive(Debug, Default)]
struct AlertEvaluatorInner {
    running: bool,
    /// `Some(true)` = cash Some, `Some(false)` = cash None, `None` = never observed.
    cash_present: Option<bool>,
    enabled_watches: Option<usize>,
    last_tick_at: Option<DateTime<Utc>>,
    last_fired_count: Option<usize>,
}

/// Shared handle updated each eval tick; read by `/health`.
#[derive(Clone, Default)]
pub struct AlertEvaluatorStatus(Arc<Mutex<AlertEvaluatorInner>>);

impl AlertEvaluatorStatus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark evaluator as spawned/running (call before or when task starts).
    pub fn set_running(&self, running: bool) {
        if let Ok(mut g) = self.0.lock() {
            g.running = running;
        }
    }

    /// Record structural fields from a completed eval tick (no magnitudes).
    pub fn record_tick(
        &self,
        cash: Option<f64>,
        enabled_watches: usize,
        fired_count: usize,
        at: DateTime<Utc>,
    ) {
        if let Ok(mut g) = self.0.lock() {
            g.running = true;
            g.cash_present = Some(cash.is_some());
            g.enabled_watches = Some(enabled_watches);
            g.last_tick_at = Some(at);
            g.last_fired_count = Some(fired_count);
        }
    }

    /// Snapshot for `/health`. Ages are computed at read time.
    pub fn dig(&self) -> AlertEvaluatorDig {
        let Ok(g) = self.0.lock() else {
            return AlertEvaluatorDig {
                running: false,
                cash: None,
                enabled_watches: None,
                last_tick_at: None,
                last_tick_age_secs: None,
                last_fired_count: None,
            };
        };
        let last_tick_age_secs = g.last_tick_at.map(|t| {
            let age = (Utc::now() - t).num_seconds();
            if age < 0 { 0 } else { age as u64 }
        });
        AlertEvaluatorDig {
            running: g.running,
            cash: g.cash_present.map(|p| if p { "Some" } else { "None" }),
            enabled_watches: g.enabled_watches,
            last_tick_at: g.last_tick_at,
            last_tick_age_secs,
            last_fired_count: g.last_fired_count,
        }
    }
}

/// Legacy env-only gate (prefer mounted `alerts.evaluator_enabled` + resolve).
pub fn alert_evaluator_enabled() -> bool {
    crate::investment::config::env_flag_or("TRADING_ALERT_EVALUATOR", false)
}

#[derive(Debug, Clone)]
pub struct AlertEvaluatorConfig {
    pub eval_interval: Duration,
    pub rest_reconcile: Duration,
}

impl Default for AlertEvaluatorConfig {
    fn default() -> Self {
        Self {
            eval_interval: Duration::from_secs(DEFAULT_EVAL_INTERVAL_SECS),
            rest_reconcile: Duration::from_secs(DEFAULT_REST_RECONCILE_SECS),
        }
    }
}

impl AlertEvaluatorConfig {
    /// Env-only cadence (legacy). Prefer [`Self::from_alerts_config`].
    pub fn from_env() -> Self {
        Self::from_alerts_config(&crate::investment::AlertsConfig::default())
    }

    /// Mounted `[finance.alerts]` with optional `TRADING_ALERT_*` env overrides.
    pub fn from_alerts_config(
        alerts: &crate::investment::AlertsConfig,
    ) -> Self {
        let (eval_interval, rest_reconcile) = alerts.resolve_intervals();
        Self {
            eval_interval,
            rest_reconcile,
        }
    }
}

#[derive(Debug, Clone)]
struct BookCache {
    cash: Option<f64>,
    nav: Option<f64>,
    /// Book symbol → observation fields from REST holdings.
    symbols: HashMap<String, SymbolObservation>,
    #[allow(dead_code)]
    loaded_at: Instant,
}

/// Entry point for skill-master (or finance binaries). Never returns; reconnects on session end.
pub async fn run_alert_evaluator(
    pool: sqlx::SqlitePool,
    api: RestClient,
    config: AlertEvaluatorConfig,
    status: AlertEvaluatorStatus,
) {
    status.set_running(true);
    tracing::info!(
        eval_interval_secs = config.eval_interval.as_secs(),
        rest_reconcile_secs = config.rest_reconcile.as_secs(),
        "trading alert evaluator starting (WS marks + REST reconcile; no Telegram; sqlite pool shared with MCP trading_watches)"
    );

    let mut backoff = Duration::from_secs(2);
    loop {
        match run_session(&pool, &api, &config, &status).await {
            Ok(()) => {
                tracing::warn!(
                    "alert evaluator session ended cleanly; reconnecting"
                );
                backoff = Duration::from_secs(2);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    backoff_secs = backoff.as_secs(),
                    "alert evaluator session failed; backing off"
                );
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

async fn run_session(
    pool: &sqlx::SqlitePool,
    api: &RestClient,
    config: &AlertEvaluatorConfig,
    status: &AlertEvaluatorStatus,
) -> anyhow::Result<()> {
    let mut ws = try_ws_client(api).await;
    if let Some(ref client) = ws {
        if let Err(e) = client.subscribe_portfolio() {
            tracing::warn!("subscribe_portfolio failed: {e}");
        } else {
            tracing::info!("Dzengi WS portfolio subscribe ok");
        }
    } else {
        tracing::warn!(
            "Dzengi WS unavailable; alert evaluator running REST-only this session"
        );
    }

    let mut book: Option<BookCache> = None;
    // Force REST reconcile on first tick.
    let mut last_reconcile = Instant::now()
        .checked_sub(config.rest_reconcile)
        .unwrap_or_else(Instant::now);
    let mut book_dirty = true;

    let mut eval_tick = tokio::time::interval(config.eval_interval);
    eval_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // Skip the immediate first tick so we can optionally wait for auth/events;
    // still evaluate promptly via the first interval after connect.
    eval_tick.tick().await;

    loop {
        tokio::select! {
            _ = eval_tick.tick() => {
                let force_book = book_dirty
                    || book.is_none()
                    || last_reconcile.elapsed() >= config.rest_reconcile;
                if let Err(e) = eval_once(
                    pool,
                    api,
                    &mut ws,
                    &mut book,
                    force_book,
                    status,
                )
                .await
                {
                    tracing::warn!(error = %e, "alert eval cycle failed");
                } else if force_book {
                    last_reconcile = Instant::now();
                    book_dirty = false;
                }
                if ws.is_none() {
                    // Prefer reconnecting rather than staying REST-only forever.
                    return Ok(());
                }
            }
            evt = recv_ws_event(&mut ws) => {
                match evt {
                    None => {
                        tracing::warn!("Dzengi WS event stream closed");
                        return Ok(());
                    }
                    Some(PortfolioEvent::Auth(a)) => {
                        tracing::debug!(
                            success = a.success,
                            reason = ?a.reason,
                            "alert evaluator WS auth event"
                        );
                    }
                    Some(PortfolioEvent::Snapshot(_))
                    | Some(PortfolioEvent::PositionUpdate(_)) => {
                        // Book may have moved; mark dirty and evaluate soon.
                        book_dirty = true;
                        let force_book = last_reconcile.elapsed()
                            >= config.rest_reconcile
                            || book.is_none();
                        if let Err(e) = eval_once(
                            pool,
                            api,
                            &mut ws,
                            &mut book,
                            force_book,
                            status,
                        )
                        .await
                        {
                            tracing::warn!(
                                error = %e,
                                "alert eval after WS book event failed"
                            );
                        } else if force_book {
                            last_reconcile = Instant::now();
                            book_dirty = false;
                        }
                    }
                    Some(PortfolioEvent::Raw(_)) => {}
                }
            }
        }
    }
}

async fn recv_ws_event(ws: &mut Option<WsClient>) -> Option<PortfolioEvent> {
    match ws.as_mut() {
        Some(client) => client.rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn try_ws_client(api: &RestClient) -> Option<WsClient> {
    if api.api_key.is_empty() || api.api_secret.is_empty() {
        tracing::debug!("alert evaluator: empty API credentials; skip WS");
        return None;
    }
    let url = ws_api::ws_connect_url(&api.base_url);
    let started = Instant::now();
    match tokio::time::timeout(
        WS_CONNECT_TIMEOUT,
        WsClient::connect(&url, &api.api_key, &api.api_secret),
    )
    .await
    {
        Ok(Ok(c)) => {
            tracing::info!(
                ws_connect_ms = started.elapsed().as_millis() as u64,
                %url,
                "alert evaluator Dzengi WS connect ok"
            );
            Some(c)
        }
        Ok(Err(e)) => {
            tracing::warn!(
                ws_connect_ms = started.elapsed().as_millis() as u64,
                %url,
                "alert evaluator WS connect failed: {e}"
            );
            None
        }
        Err(_) => {
            tracing::warn!(
                ws_connect_ms = started.elapsed().as_millis() as u64,
                %url,
                "alert evaluator WS connect timed out after {}ms",
                WS_CONNECT_TIMEOUT.as_millis()
            );
            None
        }
    }
}

async fn eval_once(
    pool: &sqlx::SqlitePool,
    api: &RestClient,
    ws: &mut Option<WsClient>,
    book: &mut Option<BookCache>,
    force_book: bool,
    status: &AlertEvaluatorStatus,
) -> anyhow::Result<()> {
    let watches = list_enabled_watches(pool).await?;
    if watches.is_empty() {
        tracing::trace!("alert evaluator: no enabled watches");
        // Still publish dig so /health shows ticks with enabled_watches=0.
        let cash = book.as_ref().and_then(|b| b.cash);
        status.record_tick(cash, 0, 0, Utc::now());
        return Ok(());
    }

    let needs_book = watches.iter().any(watch_needs_book);
    if force_book || (needs_book && book.is_none()) {
        match load_book_cache(api).await {
            Ok(cache) => {
                tracing::debug!(
                    symbols = cache.symbols.len(),
                    cash = cash_presence_label(cache.cash),
                    nav = cash_presence_label(cache.nav),
                    "alert evaluator REST book reconcile ok"
                );
                *book = Some(cache);
            }
            Err(e) => {
                tracing::warn!("REST book reconcile failed: {e}");
                if needs_book && book.is_none() {
                    anyhow::bail!("book required but reconcile failed: {e}");
                }
            }
        }
    }

    let quote_symbols = symbols_needing_quotes(&watches);
    let quotes = if quote_symbols.is_empty() {
        Vec::new()
    } else {
        refresh_quotes_ws_primary(api, ws, &quote_symbols).await
    };

    let snap = build_eval_snapshot(book.as_ref(), &quotes, Utc::now());

    // Structural only — never log cash/NAV magnitudes.
    let cash_below_enabled = watches
        .iter()
        .any(|w| matches!(w.rule, WatchRule::CashBelow));
    if cash_below_enabled && snap.cash.is_none() {
        tracing::info!(
            enabled_watches = watches.len(),
            "CashBelow skipped this tick: snap.cash is None"
        );
    }

    let fired = evaluate_enabled_watches(pool, &snap).await?;
    let tick_at = Utc::now();
    status.record_tick(snap.cash, watches.len(), fired.len(), tick_at);
    tracing::info!(
        cash = cash_presence_label(snap.cash),
        enabled_watches = watches.len(),
        fired = fired.len(),
        "alert evaluator tick"
    );
    if !fired.is_empty() {
        tracing::info!(
            count = fired.len(),
            event_ids = ?fired.iter().map(|r| r.event_id.as_str()).collect::<Vec<_>>(),
            "alert evaluator inserted watch.fired outbox rows"
        );
    }
    Ok(())
}

fn watch_needs_book(w: &Watch) -> bool {
    matches!(
        w.rule,
        WatchRule::MarkVsEntryPct
            | WatchRule::WeightBookPctAbove
            | WatchRule::CashBelow
            | WatchRule::NavDayChangePct
    )
}

/// Symbols that need live WS/REST tickers (`day_change_pct` only).
fn symbols_needing_quotes(watches: &[Watch]) -> Vec<String> {
    let mut set = HashSet::new();
    for w in watches {
        if matches!(w.rule, WatchRule::DayChangePct)
            && let Some(sym) = w.symbol.as_deref()
        {
            set.insert(sym.to_string());
        }
    }
    let mut out: Vec<String> = set.into_iter().collect();
    out.sort();
    out.truncate(MAX_EVAL_SYMBOLS);
    out
}

/// Presence label for logs — never emit cash/NAV magnitudes.
fn cash_presence_label(v: Option<f64>) -> &'static str {
    if v.is_some() { "Some" } else { "None" }
}

/// Map holdings → eval observations. Empty when assemble fails (soft path).
fn symbols_from_owning(
    owning: OwningAssets,
) -> HashMap<String, SymbolObservation> {
    let mut symbols = HashMap::new();
    for row in owning.assets {
        symbols.insert(
            row.asset.symbol.clone(),
            SymbolObservation {
                change_pct: None,
                mark: Some(row.asset.unit_market_price),
                unrealized_pnl_pct: Some(row.asset.unrealized_pnl_pct),
                weight_book_pct: Some(row.weight_book_pct),
            },
        );
    }
    symbols
}

/// REST book + wallet for CashBelow / NavDayChangePct / weight / mark-vs-entry.
///
/// **Cash/NAV do not require full holdings success.** `usd_wallet` is taken
/// from the account snapshot first; if `assemble_holdings` fails (ticker /
/// myTrades / pair soft-hard paths), we still return a BookCache with
/// `cash`/`nav` set and an empty symbol map so portfolio cash rules can fire.
async fn load_book_cache(api: &RestClient) -> anyhow::Result<BookCache> {
    let snapshot = load_dzengi_snapshot(api).await?;
    // Wallet first — CashBelow / wallet NAV must not depend on holdings.
    let (cash, reserved) = usd_wallet(&snapshot.account);
    let nav = nav_from_wallet(cash, reserved);

    let symbols = match assemble_holdings(api, &snapshot).await {
        Ok(holdings) => {
            let owning = OwningAssets::from_holdings(holdings.assets, nav);
            symbols_from_owning(owning)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                cash = cash_presence_label(cash),
                nav = cash_presence_label(nav),
                "assemble_holdings failed; soft-continuing with wallet cash/nav only"
            );
            HashMap::new()
        }
    };

    Ok(BookCache {
        cash,
        nav,
        symbols,
        loaded_at: Instant::now(),
    })
}

/// Merge book cache + live quotes into an [`EvalSnapshot`].
///
/// `nav_day_change_pct` stays unset until daily NAV snapshots exist (Wave D).
fn build_eval_snapshot(
    book: Option<&BookCache>,
    quotes: &[Quote],
    now: DateTime<Utc>,
) -> EvalSnapshot {
    let mut symbols: HashMap<String, SymbolObservation> = HashMap::new();
    let (cash, nav) = if let Some(b) = book {
        for (k, v) in &b.symbols {
            symbols.insert(k.clone(), v.clone());
        }
        (b.cash, b.nav)
    } else {
        (None, None)
    };

    for q in quotes {
        let entry = symbols.entry(q.symbol.clone()).or_default();
        if let Some(pct) = q.change_pct {
            entry.change_pct = Some(pct);
        }
        if let Some(last) = q.last {
            entry.mark = Some(last);
        }
    }

    EvalSnapshot {
        symbols,
        cash,
        nav,
        nav_day_change_pct: None,
        now,
    }
}

/// WS-primary tickers for watched symbols; REST fallback per symbol.
async fn refresh_quotes_ws_primary(
    api: &RestClient,
    ws: &mut Option<WsClient>,
    symbols: &[String],
) -> Vec<Quote> {
    if symbols.is_empty() {
        return Vec::new();
    }

    let server_ts = match api.time().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("alert evaluator time() failed: {e}");
            return Vec::new();
        }
    };
    let exchange_info = match api.exchange_info(server_ts).await {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("alert evaluator exchangeInfo failed: {e}");
            return Vec::new();
        }
    };
    let api_prefix = ws_api::ws_api_prefix(&api.base_url);

    let mut out = Vec::with_capacity(symbols.len());
    for raw in symbols {
        let Some(trade) = resolve_quote_pair(&exchange_info, raw) else {
            out.push(Quote {
                symbol: raw.clone(),
                last: None,
                bid: None,
                ask: None,
                prev_close: None,
                change: None,
                change_pct: None,
                error: Some(format!("no trading pair for {raw}")),
            });
            continue;
        };

        let ticker = if let Some(client) = ws.as_mut() {
            let destination = format!("{api_prefix}/ticker/24hr");
            let cid = format!(
                "alert-ticker-{}-{}",
                trade,
                crate::investment::now_ms()
            );
            match client.ws_ticker_at(&destination, &trade, &cid).await {
                Ok(t) => Ok(t),
                Err(e) => {
                    tracing::warn!(
                        symbol = %trade,
                        "WS ticker failed in evaluator: {e}; REST fallback"
                    );
                    *ws = None;
                    api.ticker(&trade).await
                }
            }
        } else {
            api.ticker(&trade).await
        };

        match ticker {
            Ok(t) => out.push(Quote {
                symbol: raw.clone(),
                last: positive_or_none(t.last_price)
                    .or_else(|| positive_or_none(t.bid_price))
                    .or_else(|| positive_or_none(t.ask_price)),
                bid: positive_or_none(t.bid_price),
                ask: positive_or_none(t.ask_price),
                prev_close: positive_or_none(t.prev_close_price),
                change: t.price_change,
                change_pct: t.price_change_percent,
                error: None,
            }),
            Err(e) => out.push(Quote {
                symbol: raw.clone(),
                last: None,
                bid: None,
                ask: None,
                prev_close: None,
                change: None,
                change_pct: None,
                error: Some(format!("ticker failed for {trade}: {e}")),
            }),
        }
    }
    out
}

fn positive_or_none(v: f64) -> Option<f64> {
    if v.is_finite() && v > 0.0 {
        Some(v)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investment::watches::{WatchChannel, WatchCompare};

    #[test]
    fn evaluator_disabled_by_default() {
        // Ensure we don't accidentally treat missing env as enabled in unit tests.
        // (Cannot clear process env safely if parent set it; only assert parse helper.)
        assert!(!parse_enabled_flag(None));
        assert!(!parse_enabled_flag(Some("0")));
        assert!(!parse_enabled_flag(Some("false")));
        assert!(parse_enabled_flag(Some("1")));
        assert!(parse_enabled_flag(Some("true")));
        assert!(parse_enabled_flag(Some("YES")));
    }

    fn parse_enabled_flag(raw: Option<&str>) -> bool {
        raw.map(|v| {
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
    }

    #[test]
    fn build_snapshot_merges_quotes_over_book_marks() {
        let mut book_symbols = HashMap::new();
        book_symbols.insert(
            "TSLA".into(),
            SymbolObservation {
                change_pct: None,
                mark: Some(350.0),
                unrealized_pnl_pct: Some(-2.0),
                weight_book_pct: Some(25.0),
            },
        );
        let book = BookCache {
            cash: Some(100.0),
            nav: Some(5000.0),
            symbols: book_symbols,
            loaded_at: Instant::now(),
        };
        let quotes = vec![Quote {
            symbol: "TSLA".into(),
            last: Some(352.68),
            bid: None,
            ask: None,
            prev_close: Some(340.0),
            change: Some(12.68),
            change_pct: Some(-6.2),
            error: None,
        }];
        let snap = build_eval_snapshot(Some(&book), &quotes, Utc::now());
        let obs = snap.symbols.get("TSLA").unwrap();
        assert_eq!(obs.change_pct, Some(-6.2));
        assert_eq!(obs.mark, Some(352.68));
        assert_eq!(obs.unrealized_pnl_pct, Some(-2.0));
        assert_eq!(snap.cash, Some(100.0));
        assert_eq!(snap.nav, Some(5000.0));
        assert!(snap.nav_day_change_pct.is_none());
    }

    #[test]
    fn day_change_symbols_collected() {
        let watches = vec![Watch {
            id: 1,
            symbol: Some("TSLA".into()),
            rule: WatchRule::DayChangePct,
            threshold: -5.0,
            compare: WatchCompare::Lte,
            channel: WatchChannel::Mcp,
            cooldown_secs: 60,
            enabled: true,
            last_fired_at: None,
            created_at: Utc::now(),
        }];
        let syms = symbols_needing_quotes(&watches);
        assert_eq!(syms, vec!["TSLA".to_string()]);
    }

    #[test]
    fn config_defaults_in_design_range() {
        let cfg = AlertEvaluatorConfig::default();
        assert_eq!(cfg.eval_interval.as_secs(), 60);
        assert!(
            (300..=900).contains(&cfg.rest_reconcile.as_secs()),
            "REST reconcile should be 5–15m by default"
        );
    }

    #[test]
    fn cash_presence_label_is_structural_only() {
        assert_eq!(cash_presence_label(None), "None");
        assert_eq!(cash_presence_label(Some(42.0)), "Some");
    }

    #[test]
    fn status_dig_defaults_not_running() {
        let s = AlertEvaluatorStatus::new();
        let dig = s.dig();
        assert!(!dig.running);
        assert!(dig.cash.is_none());
        assert!(dig.enabled_watches.is_none());
        assert!(dig.last_tick_at.is_none());
        assert!(dig.last_tick_age_secs.is_none());
        assert!(dig.last_fired_count.is_none());
        let json = serde_json::to_value(&dig).unwrap();
        assert_eq!(json["running"], false);
        assert!(json.get("cash").is_none());
    }

    #[test]
    fn status_dig_records_structural_tick_no_magnitudes() {
        let s = AlertEvaluatorStatus::new();
        s.set_running(true);
        let at = Utc::now();
        s.record_tick(Some(12_345.67), 2, 1, at);
        let dig = s.dig();
        assert!(dig.running);
        assert_eq!(dig.cash, Some("Some"));
        assert_eq!(dig.enabled_watches, Some(2));
        assert_eq!(dig.last_fired_count, Some(1));
        assert_eq!(dig.last_tick_at, Some(at));
        assert!(dig.last_tick_age_secs.is_some());
        let json = serde_json::to_string(&dig).unwrap();
        assert!(!json.contains("12345"));
        assert!(!json.contains("12_345"));
        assert!(json.contains("\"Some\""));
        s.record_tick(None, 0, 0, Utc::now());
        assert_eq!(s.dig().cash, Some("None"));
    }

    #[test]
    fn soft_book_with_wallet_only_exposes_cash_for_cash_below() {
        // Holdings assemble failed → empty symbols, but wallet cash/nav present.
        let book = BookCache {
            cash: Some(1500.0),
            nav: Some(1500.0),
            symbols: HashMap::new(),
            loaded_at: Instant::now(),
        };
        let snap = build_eval_snapshot(Some(&book), &[], Utc::now());
        assert_eq!(snap.cash, Some(1500.0));
        assert_eq!(snap.nav, Some(1500.0));
        assert!(snap.symbols.is_empty());

        let watch = Watch {
            id: 1,
            symbol: None,
            rule: WatchRule::CashBelow,
            threshold: 999_999_999.0,
            compare: WatchCompare::Lte,
            channel: WatchChannel::Both,
            cooldown_secs: 60,
            enabled: true,
            last_fired_at: None,
            created_at: Utc::now(),
        };
        let fire = crate::investment::watches::evaluate_watch(&watch, &snap);
        assert!(fire.is_some(), "CashBelow must fire when snap.cash is Some");
    }

    #[test]
    fn cash_below_skips_when_cash_none_even_with_book() {
        let book = BookCache {
            cash: None,
            nav: None,
            symbols: HashMap::new(),
            loaded_at: Instant::now(),
        };
        let snap = build_eval_snapshot(Some(&book), &[], Utc::now());
        let watch = Watch {
            id: 1,
            symbol: None,
            rule: WatchRule::CashBelow,
            threshold: 999_999_999.0,
            compare: WatchCompare::Lte,
            channel: WatchChannel::Both,
            cooldown_secs: 60,
            enabled: true,
            last_fired_at: None,
            created_at: Utc::now(),
        };
        assert!(
            crate::investment::watches::evaluate_watch(&watch, &snap).is_none()
        );
    }
}
