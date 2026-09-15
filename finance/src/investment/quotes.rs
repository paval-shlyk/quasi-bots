//! Live marks / day moves for named symbols (REST-primary, optional short-budget WS).

use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};

use crate::investment::model::{ExchangeInfo, Ticker};
use crate::investment::rest_api::RestClient;
use crate::investment::routes::resolve_trade_pair;
use crate::investment::ws_api::{self, Client as WsClient};

/// Soft cap to protect broker rate limits (design A1).
pub const MAX_QUOTE_SYMBOLS: usize = 30;

/// Re-export WS budgets used by quotes (issue #22).
pub use ws_api::{WS_CONNECT_TIMEOUT, WS_REQUEST_TIMEOUT_SECS};

/// Overall `fetch_quotes` wall clock — must stay under MCP tool timeout (~30–60s).
pub const FETCH_QUOTES_DEADLINE: Duration = Duration::from_secs(20);

/// One symbol quote row. Soft-fail via `error`; other fields may be null.
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Quote {
    pub symbol: String,
    pub last: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub prev_close: Option<f64>,
    pub change: Option<f64>,
    pub change_pct: Option<f64>,
    pub error: Option<String>,
}

/// Batch response for `trading_quotes`.
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct QuotesResponse {
    pub as_of: DateTime<Utc>,
    pub quotes: Vec<Quote>,
}

impl Quote {
    fn ok(symbol: String, ticker: &Ticker) -> Self {
        Self {
            symbol,
            last: positive_or_none(ticker.last_price),
            bid: positive_or_none(ticker.bid_price),
            ask: positive_or_none(ticker.ask_price),
            prev_close: positive_or_none(ticker.prev_close_price),
            // Do not invent day moves when Dzengi omitted the fields.
            change: ticker.price_change,
            change_pct: ticker.price_change_percent,
            error: None,
        }
    }

    fn err(symbol: String, error: impl Into<String>) -> Self {
        Self {
            symbol,
            last: None,
            bid: None,
            ask: None,
            prev_close: None,
            change: None,
            change_pct: None,
            error: Some(error.into()),
        }
    }
}

fn positive_or_none(v: f64) -> Option<f64> {
    // Dzengi often emits 0.0 for missing bid/ask/last; treat non-positive as null.
    if v.is_finite() && v > 0.0 {
        Some(v)
    } else {
        None
    }
}

fn normalize_input_symbol(symbol: &str) -> String {
    symbol
        .strip_suffix('.')
        .unwrap_or(symbol)
        .trim()
        .to_string()
}

/// Resolve a user/book symbol to a Dzengi trade pair (same venue as positions).
pub fn resolve_quote_pair(info: &ExchangeInfo, symbol: &str) -> Option<String> {
    let normalized = normalize_input_symbol(symbol);
    if normalized.is_empty() {
        return None;
    }

    // Exact trade-pair match (e.g. `TON/USD_LEVERAGE`, `BTC/USD`).
    if let Some(s) = info.symbols.iter().find(|s| s.symbol == symbol) {
        return Some(s.symbol.clone());
    }
    if let Some(s) = info.symbols.iter().find(|s| s.symbol == normalized) {
        return Some(s.symbol.clone());
    }

    resolve_trade_pair(info, &normalized)
        .or_else(|| resolve_trade_pair(info, symbol))
        .map(|(pair, _)| pair)
}

/// `QUOTES_TRANSPORT=ws` opts into short-budget WS-primary; default is REST (#22 A1).
fn quotes_prefer_ws() -> bool {
    std::env::var("QUOTES_TRANSPORT")
        .map(|v| v.eq_ignore_ascii_case("ws"))
        .unwrap_or(false)
}

async fn try_ws_client(api: &RestClient) -> Option<WsClient> {
    if api.api_key.is_empty() || api.api_secret.is_empty() {
        tracing::debug!("skipping WS quotes: empty API credentials");
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
                "Dzengi WS connect ok"
            );
            Some(c)
        }
        Ok(Err(e)) => {
            tracing::warn!(
                ws_connect_ms = started.elapsed().as_millis() as u64,
                %url,
                "Dzengi WS connect failed: {e}; REST fallback"
            );
            None
        }
        Err(_) => {
            tracing::warn!(
                ws_connect_ms = started.elapsed().as_millis() as u64,
                %url,
                "Dzengi WS connect timed out after {}ms; REST fallback",
                WS_CONNECT_TIMEOUT.as_millis()
            );
            None
        }
    }
}

async fn ticker_via_ws(
    ws: &mut WsClient,
    api_prefix: &str,
    trade_symbol: &str,
) -> anyhow::Result<Ticker> {
    let destination = format!("{api_prefix}/ticker/24hr");
    let cid =
        format!("ticker-{}-{}", trade_symbol, crate::investment::now_ms());
    let started = Instant::now();
    let result = ws.ws_ticker_at(&destination, trade_symbol, &cid).await;
    tracing::debug!(
        ws_ticker_ms = started.elapsed().as_millis() as u64,
        symbol = %trade_symbol,
        ok = result.is_ok(),
        "WS ticker attempt"
    );
    result
}

async fn ticker_via_rest(
    api: &RestClient,
    trade_symbol: &str,
) -> anyhow::Result<Ticker> {
    let started = Instant::now();
    let result = api.ticker(trade_symbol).await;
    tracing::info!(
        rest_ticker_ms = started.elapsed().as_millis() as u64,
        symbol = %trade_symbol,
        ok = result.is_ok(),
        "REST ticker"
    );
    result
}

async fn fetch_one_ticker(
    api: &RestClient,
    ws: &mut Option<WsClient>,
    api_prefix: &str,
    trade_symbol: &str,
    prefer_ws: bool,
) -> anyhow::Result<Ticker> {
    if prefer_ws && let Some(client) = ws.as_mut() {
        match ticker_via_ws(client, api_prefix, trade_symbol).await {
            Ok(t) => return Ok(t),
            Err(e) => {
                tracing::warn!(
                    "WS ticker failed for {trade_symbol}: {e}; trying REST"
                );
                // Drop the socket after a hard failure so later symbols use REST
                // rather than repeatedly timing out on a broken connection.
                *ws = None;
            }
        }
    }
    ticker_via_rest(api, trade_symbol).await
}

/// Fetch marks / day moves for `symbols` (REST-primary by default; WS optional).
///
/// Hard-fails only on empty / oversized input or exchangeInfo load failure.
/// Per-symbol mapping / ticker failures become soft `error` rows.
/// Wall-clock capped by [`FETCH_QUOTES_DEADLINE`] so the MCP tool never -32001s.
pub async fn fetch_quotes(
    api: &RestClient,
    symbols: &[String],
) -> anyhow::Result<QuotesResponse> {
    if symbols.is_empty() {
        anyhow::bail!("trading_quotes requires at least one symbol");
    }
    if symbols.len() > MAX_QUOTE_SYMBOLS {
        anyhow::bail!(
            "trading_quotes accepts at most {MAX_QUOTE_SYMBOLS} symbols (got {})",
            symbols.len()
        );
    }

    let deadline = Instant::now() + FETCH_QUOTES_DEADLINE;
    fetch_quotes_within(api, symbols, deadline).await
}

async fn fetch_quotes_within(
    api: &RestClient,
    symbols: &[String],
    deadline: Instant,
) -> anyhow::Result<QuotesResponse> {
    let server_ts = api.time().await?;
    let exchange_info = api.exchange_info(server_ts).await?;
    let api_prefix = ws_api::ws_api_prefix(&api.base_url);
    let prefer_ws = quotes_prefer_ws();
    let mut ws = if prefer_ws && Instant::now() < deadline {
        try_ws_client(api).await
    } else {
        None
    };

    let mut quotes = Vec::with_capacity(symbols.len());
    for raw in symbols {
        if Instant::now() >= deadline {
            let symbol = normalize_input_symbol(raw);
            quotes.push(Quote::err(
                if symbol.is_empty() {
                    raw.clone()
                } else {
                    symbol
                },
                format!(
                    "trading_quotes deadline ({}ms) exceeded; try fewer symbols",
                    FETCH_QUOTES_DEADLINE.as_millis()
                ),
            ));
            continue;
        }

        let symbol = normalize_input_symbol(raw);
        if symbol.is_empty() {
            quotes.push(Quote::err(raw.clone(), "empty symbol"));
            continue;
        }

        let Some(trade_symbol) = resolve_quote_pair(&exchange_info, &symbol)
        else {
            quotes.push(Quote::err(
                symbol,
                format!("no trading pair found for symbol {raw}"),
            ));
            continue;
        };

        match fetch_one_ticker(
            api,
            &mut ws,
            api_prefix,
            &trade_symbol,
            prefer_ws,
        )
        .await
        {
            Ok(ticker) => quotes.push(Quote::ok(symbol, &ticker)),
            Err(e) => quotes.push(Quote::err(
                symbol,
                format!("ticker failed for {trade_symbol}: {e}"),
            )),
        }
    }

    Ok(QuotesResponse {
        as_of: Utc::now(),
        quotes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investment::model::SymbolInfo;

    fn symbol_info(symbol: &str, base: &str, quote: &str) -> SymbolInfo {
        SymbolInfo {
            symbol: symbol.into(),
            status: "TRADING".into(),
            base_asset: base.into(),
            quote_asset: quote.into(),
            asset_type: Some("EQUITY".into()),
            market_type: Some("LEVERAGE".into()),
        }
    }

    fn exchange_info(symbols: Vec<SymbolInfo>) -> ExchangeInfo {
        ExchangeInfo {
            timezone: None,
            server_time: None,
            symbols,
        }
    }

    fn sample_ticker(symbol: &str) -> Ticker {
        Ticker {
            symbol: symbol.into(),
            price_change: Some(12.68),
            price_change_percent: Some(3.73),
            weighted_avg_price: 345.0,
            prev_close_price: 340.0,
            last_price: 352.68,
            last_qty: 1.0,
            bid_price: 352.5,
            ask_price: 352.8,
            open_price: 340.0,
            high_price: 355.0,
            low_price: 338.0,
            volume: 1000.0,
            quote_volume: 350000.0,
            open_time: DateTime::from_timestamp_millis(0).unwrap(),
            close_time: DateTime::from_timestamp_millis(0).unwrap(),
        }
    }

    #[test]
    fn given_base_asset_when_resolve_quote_pair_then_uses_resolve_trade_pair() {
        let info = exchange_info(vec![
            symbol_info("TSLA/USD_LEVERAGE", "TSLA", "USD"),
            symbol_info("Gold/USD", "Gold", "USD"),
            symbol_info("US500/USD_LEVERAGE", "US500", "USD"),
        ]);
        assert_eq!(
            resolve_quote_pair(&info, "TSLA").as_deref(),
            Some("TSLA/USD_LEVERAGE")
        );
        assert_eq!(
            resolve_quote_pair(&info, "Gold").as_deref(),
            Some("Gold/USD")
        );
        assert_eq!(
            resolve_quote_pair(&info, "US500").as_deref(),
            Some("US500/USD_LEVERAGE")
        );
    }

    #[test]
    fn given_exact_pair_when_resolve_quote_pair_then_returns_same() {
        let info =
            exchange_info(vec![symbol_info("TON/USD_LEVERAGE", "TON", "USD")]);
        assert_eq!(
            resolve_quote_pair(&info, "TON/USD_LEVERAGE").as_deref(),
            Some("TON/USD_LEVERAGE")
        );
    }

    #[test]
    fn given_bare_index_symbol_when_resolve_quote_pair_then_exact_pair() {
        // Dzengi often lists index CFDs as symbol==base (e.g. US500 / Gold).
        let info = exchange_info(vec![
            symbol_info("US500", "US500", "USD"),
            symbol_info("Gold", "Gold", "USD"),
        ]);
        assert_eq!(
            resolve_quote_pair(&info, "US500").as_deref(),
            Some("US500")
        );
        assert_eq!(resolve_quote_pair(&info, "Gold").as_deref(), Some("Gold"));
        assert_eq!(resolve_quote_pair(&info, "Gold.").as_deref(), Some("Gold"));
    }

    #[test]
    fn given_unknown_when_resolve_quote_pair_then_none() {
        let info = exchange_info(vec![symbol_info("BTC/USD", "BTC", "USD")]);
        assert!(resolve_quote_pair(&info, "NOPE").is_none());
    }

    #[test]
    fn given_ticker_when_quote_ok_then_maps_fields() {
        let q = Quote::ok("TSLA".into(), &sample_ticker("TSLA/USD_LEVERAGE"));
        assert_eq!(q.symbol, "TSLA");
        assert_eq!(q.last, Some(352.68));
        assert_eq!(q.bid, Some(352.5));
        assert_eq!(q.ask, Some(352.8));
        assert_eq!(q.prev_close, Some(340.0));
        assert_eq!(q.change, Some(12.68));
        assert_eq!(q.change_pct, Some(3.73));
        assert!(q.error.is_none());
    }

    #[test]
    fn given_partial_ticker_when_quote_ok_then_null_day_change() {
        let mut t = sample_ticker("US500");
        t.price_change = None;
        t.price_change_percent = None;
        let q = Quote::ok("US500".into(), &t);
        assert_eq!(q.last, Some(352.68));
        assert!(q.change.is_none());
        assert!(q.change_pct.is_none());
    }

    #[test]
    fn given_error_when_quote_err_then_nulls_marks() {
        let q = Quote::err("NOPE".into(), "no trading pair found");
        assert_eq!(q.symbol, "NOPE");
        assert!(q.last.is_none());
        assert_eq!(q.error.as_deref(), Some("no trading pair found"));
    }

    #[test]
    fn quotes_response_serializes_as_of_and_nullable_fields() {
        let resp = QuotesResponse {
            as_of: DateTime::from_timestamp(1_725_667_200, 0).unwrap(),
            quotes: vec![
                Quote::ok("TSLA".into(), &sample_ticker("TSLA")),
                Quote::err(
                    "NOPE".into(),
                    "no trading pair found for symbol NOPE",
                ),
            ],
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v.get("as_of").is_some());
        assert_eq!(v["quotes"][0]["symbol"], "TSLA");
        assert_eq!(v["quotes"][0]["last"], 352.68);
        assert!(v["quotes"][0]["error"].is_null());
        assert!(v["quotes"][1]["last"].is_null());
        assert_eq!(
            v["quotes"][1]["error"],
            "no trading pair found for symbol NOPE"
        );
    }

    #[test]
    fn timeout_constants_leave_mcp_headroom() {
        const {
            assert!(WS_CONNECT_TIMEOUT.as_secs() <= 3);
            assert!(WS_REQUEST_TIMEOUT_SECS <= 3);
            assert!(FETCH_QUOTES_DEADLINE.as_secs() <= 25);
        };
        assert!(FETCH_QUOTES_DEADLINE > WS_CONNECT_TIMEOUT);
    }

    #[tokio::test]
    async fn given_empty_symbols_when_fetch_quotes_then_errors() {
        let api = RestClient::new("http://127.0.0.1:9", "", "");
        let err = fetch_quotes(&api, &[]).await.unwrap_err();
        assert!(
            err.to_string().contains("at least one"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn given_too_many_symbols_when_fetch_quotes_then_errors() {
        let api = RestClient::new("http://127.0.0.1:9", "", "");
        let symbols: Vec<String> =
            (0..=MAX_QUOTE_SYMBOLS).map(|i| format!("S{i}")).collect();
        let err = fetch_quotes(&api, &symbols).await.unwrap_err();
        assert!(
            err.to_string().contains("at most"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn given_ws_connect_hang_when_try_ws_client_then_none_fast() {
        // Unroutable blackhole: connect must not hang past WS_CONNECT_TIMEOUT.
        let api =
            RestClient::new("https://240.0.0.1:9/api/v2", "key", "secret");
        let started = Instant::now();
        let client = try_ws_client(&api).await;
        let elapsed = started.elapsed();
        assert!(client.is_none());
        assert!(
            elapsed < WS_CONNECT_TIMEOUT + Duration::from_secs(2),
            "WS connect budget exceeded: {elapsed:?}"
        );
    }
}
