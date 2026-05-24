use chrono::{DateTime, TimeZone, Utc};
use futures_util::StreamExt;
use rust_decimal::Decimal;
use tokio::sync::watch;
use tokio_tungstenite::connect_async;

use crate::error::{CryptoError, Result};

use super::models::{Candle, MarketData};


const BINANCE_WS_BASE: &str = "wss://stream.binance.com:9443/ws";

/// Converts a pair like "BTC/USDC" to the Binance stream name like "btcusdc".
fn pair_to_binance_symbol(pair: &str) -> String {
    pair.replace('/', "").to_lowercase()
}

/// Subscribes to Binance WebSocket ticker + kline streams for the given pairs.
/// Pushes updates into the returned receivers.
///
/// Returns a `watch::Receiver<Vec<MarketData>>` for ticker snapshots and
/// a `watch::Receiver<HashMap<String, Vec<Candle>>>` for candle history.
/// The task runs until the provided shutdown token fires.
pub fn spawn_market_feed(
    pairs: Vec<String>,
    candle_interval: &str,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> (
    watch::Receiver<Vec<MarketData>>,
    watch::Receiver<std::collections::HashMap<String, Vec<Candle>>>,
) {
    let (ticker_tx, ticker_rx) = watch::channel(Vec::new());
    let (candle_tx, candle_rx) = watch::channel(std::collections::HashMap::new());

    let interval = candle_interval.to_string();
    tokio::spawn(async move {
        if let Err(e) = run_feed(pairs, &interval, ticker_tx, candle_tx, shutdown).await {
            tracing::error!(error = %e, "Market feed terminated with error");
        }
    });

    (ticker_rx, candle_rx)
}

async fn run_feed(
    pairs: Vec<String>,
    candle_interval: &str,
    ticker_tx: watch::Sender<Vec<MarketData>>,
    candle_tx: watch::Sender<std::collections::HashMap<String, Vec<Candle>>>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let streams: Vec<String> = pairs
        .iter()
        .flat_map(|p| {
            let sym = pair_to_binance_symbol(p);
            vec![
                format!("{sym}@ticker"),
                format!("{sym}@kline_{candle_interval}"),
            ]
        })
        .collect();

    let stream_param = streams.join("/");
    let url = format!("{BINANCE_WS_BASE}/{stream_param}");
    tracing::info!(url = %url, "Connecting to Binance WebSocket");

    let (ws_stream, _) = connect_async(&url)
        .await
        .map_err(|e| CryptoError::Trading(format!("WebSocket connect failed: {e}")))?;

    let (_, mut read) = ws_stream.split();

    // State: latest ticker per pair, candle history per pair
    let pair_lookup: std::collections::HashMap<String, String> = pairs
        .iter()
        .map(|p| (pair_to_binance_symbol(p), p.clone()))
        .collect();

    let mut tickers: std::collections::HashMap<String, MarketData> =
        std::collections::HashMap::new();
    let mut candles: std::collections::HashMap<String, Vec<Candle>> =
        std::collections::HashMap::new();

    loop {
        tokio::select! {
            msg = read.next() => {
                let Some(msg) = msg else {
                    tracing::warn!("WebSocket stream ended");
                    break;
                };
                let msg = match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => t,
                    Ok(tokio_tungstenite::tungstenite::Message::Ping(_)) => continue,
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                        tracing::warn!("WebSocket closed by server");
                        break;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "WebSocket read error");
                        break;
                    }
                    _ => continue,
                };

                let v: serde_json::Value = match serde_json::from_str(&msg) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let event = v["e"].as_str().unwrap_or("");
                let raw_symbol = v["s"].as_str().unwrap_or("").to_lowercase();
                let pair = match pair_lookup.get(&raw_symbol) {
                    Some(p) => p.clone(),
                    None => continue,
                };

                match event {
                    "24hrTicker" => {
                        if let Some(md) = parse_ticker(&v, &pair) {
                            tickers.insert(pair, md);
                            let snapshot: Vec<MarketData> = tickers.values().cloned().collect();
                            let _ = ticker_tx.send(snapshot);
                        }
                    }
                    "kline" => {
                        if let Some(candle) = parse_kline(&v) {
                            let is_closed = v["k"]["x"].as_bool().unwrap_or(false);
                            if is_closed {
                                let entry = candles.entry(pair).or_default();
                                entry.push(candle);
                                // Keep a rolling window of 200 candles
                                if entry.len() > 200 {
                                    entry.drain(..entry.len() - 200);
                                }
                                let _ = candle_tx.send(candles.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ = shutdown.changed() => {
                tracing::info!("Market feed shutting down");
                break;
            }
        }
    }

    Ok(())
}

fn parse_ticker(v: &serde_json::Value, pair: &str) -> Option<MarketData> {
    Some(MarketData {
        pair: pair.to_string(),
        price: parse_dec(v["c"].as_str()?)?,
        bid: parse_dec(v["b"].as_str()?)?,
        ask: parse_dec(v["a"].as_str()?)?,
        volume_24h: parse_dec(v["v"].as_str()?)?,
        high_24h: parse_dec(v["h"].as_str()?)?,
        low_24h: parse_dec(v["l"].as_str()?)?,
        timestamp: Utc::now(),
    })
}

fn parse_kline(v: &serde_json::Value) -> Option<Candle> {
    let k = &v["k"];
    let ts_ms = k["t"].as_i64()?;
    let timestamp: DateTime<Utc> = Utc.timestamp_millis_opt(ts_ms).single()?;

    Some(Candle {
        open: parse_dec(k["o"].as_str()?)?,
        high: parse_dec(k["h"].as_str()?)?,
        low: parse_dec(k["l"].as_str()?)?,
        close: parse_dec(k["c"].as_str()?)?,
        volume: parse_dec(k["v"].as_str()?)?,
        timestamp,
    })
}

fn parse_dec(s: &str) -> Option<Decimal> {
    s.parse().ok()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_conversion() {
        assert_eq!(pair_to_binance_symbol("BTC/USDC"), "btcusdc");
        assert_eq!(pair_to_binance_symbol("ETH/USDT"), "ethusdt");
    }

    #[test]
    fn parse_ticker_json() {
        let v = serde_json::json!({
            "e": "24hrTicker",
            "s": "BTCUSDC",
            "c": "50000.50",
            "b": "49999.00",
            "a": "50001.00",
            "v": "12345.67",
            "h": "51000.00",
            "l": "49000.00",
        });
        let md = parse_ticker(&v, "BTC/USDC").unwrap();
        assert_eq!(md.pair, "BTC/USDC");
        assert_eq!(md.price, Decimal::new(5000050, 2));
    }

    #[test]
    fn parse_kline_json() {
        let v = serde_json::json!({
            "e": "kline",
            "k": {
                "t": 1700000000000_i64,
                "o": "100.0",
                "h": "105.0",
                "l": "99.0",
                "c": "103.0",
                "v": "500.0",
                "x": true,
            }
        });
        let candle = parse_kline(&v).unwrap();
        assert_eq!(candle.close, Decimal::new(103, 0));
    }
}
