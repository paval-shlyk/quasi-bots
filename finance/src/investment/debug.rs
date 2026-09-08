//! Read-only finance debug probes for local/`FINANCE_DEBUG=1` only.
//!
//! Never mount these HTTP routes in production without the env gate. All
//! helpers strip secrets (`apiKey`, `signature`, …) before returning JSON.

use serde_json::{Value, json};

use crate::investment::now_ms;
use crate::investment::quotes;
use crate::investment::rest_api::RestClient;
use crate::investment::ws_api::{self, Client as WsClient};

/// `true` in debug builds, or when `FINANCE_DEBUG=1` / `true` (case-insensitive).
pub fn finance_debug_enabled() -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    std::env::var("FINANCE_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Recursively redact secret-looking keys in a JSON value.
pub fn redact_secrets(mut value: Value) -> Value {
    redact_value(&mut value);
    value
}

fn redact_value(v: &mut Value) {
    match v {
        Value::Object(map) => {
            const SECRET_KEYS: &[&str] = &[
                "apiKey",
                "api_key",
                "apiSecret",
                "api_secret",
                "signature",
                "Authorization",
                "authorization",
                "token",
                "password",
                "secret",
            ];
            for (k, child) in map.iter_mut() {
                if SECRET_KEYS.iter().any(|s| k.eq_ignore_ascii_case(s)) {
                    *child = Value::String("[redacted]".into());
                } else {
                    redact_value(child);
                }
            }
        }
        Value::Array(arr) => {
            for child in arr {
                redact_value(child);
            }
        }
        _ => {}
    }
}

/// Known WS destinations / channels used by trading quotes + portfolio.
pub fn ws_topics(rest_base_url: &str) -> Value {
    let prefix = ws_api::ws_api_prefix(rest_base_url);
    json!({
        "connect_url": ws_api::ws_connect_url(rest_base_url),
        "api_prefix": prefix,
        "destinations": [
            format!("{prefix}/auth"),
            format!("{prefix}/time"),
            format!("{prefix}/ticker/24hr"),
            format!("{prefix}/depth"),
            format!("{prefix}/exchangeInfo"),
            format!("{prefix}/account"),
            format!("{prefix}/deposits"),
            format!("{prefix}/myTrades"),
        ],
        "subscribe_channels": ["portfolio"],
        "notes": "Read-only probe. Auth destination requires signed payload; never returned here."
    })
}

/// REST `ticker/24hr` sample for one trade symbol (no secrets in response body).
pub async fn rest_ticker_sample(
    api: &RestClient,
    symbol: &str,
) -> anyhow::Result<Value> {
    let trade = resolve_or_passthrough(api, symbol).await?;
    let ticker = api.ticker(&trade).await?;
    Ok(json!({
        "source": "rest",
        "input_symbol": symbol,
        "trade_symbol": trade,
        "ticker": ticker,
    }))
}

/// One-shot WS ticker request; returns the **redacted** raw frame.
pub async fn ws_ticker_frame_redacted(
    api: &RestClient,
    symbol: &str,
) -> anyhow::Result<Value> {
    if api.api_key.is_empty() || api.api_secret.is_empty() {
        anyhow::bail!("WS debug probe requires API credentials");
    }
    let trade = resolve_or_passthrough(api, symbol).await?;
    let url = ws_api::ws_connect_url(&api.base_url);
    let prefix = ws_api::ws_api_prefix(&api.base_url);
    let mut ws = WsClient::connect(&url, &api.api_key, &api.api_secret).await?;
    let destination = format!("{prefix}/ticker/24hr");
    let cid = format!("debug-ticker-{}-{}", trade, now_ms());
    let payload = json!({"symbol": trade});
    let frame = ws.request(&destination, payload, &cid, 8).await?;
    Ok(json!({
        "source": "ws",
        "input_symbol": symbol,
        "trade_symbol": trade,
        "destination": destination,
        "frame": redact_secrets(frame),
    }))
}

async fn resolve_or_passthrough(
    api: &RestClient,
    symbol: &str,
) -> anyhow::Result<String> {
    let server_ts = api.time().await?;
    let info = api.exchange_info(server_ts).await?;
    Ok(quotes::resolve_quote_pair(&info, symbol)
        .unwrap_or_else(|| symbol.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_secrets_strips_api_key_and_signature() {
        let v = json!({
            "apiKey": "secret-key",
            "payload": {"signature": "deadbeef", "symbol": "TSLA"},
            "nested": [{"api_secret": "x", "ok": 1}]
        });
        let r = redact_secrets(v);
        assert_eq!(r["apiKey"], "[redacted]");
        assert_eq!(r["payload"]["signature"], "[redacted]");
        assert_eq!(r["payload"]["symbol"], "TSLA");
        assert_eq!(r["nested"][0]["api_secret"], "[redacted]");
        assert_eq!(r["nested"][0]["ok"], 1);
    }

    #[test]
    fn ws_topics_lists_ticker_destination() {
        let v = ws_topics("https://api-adapter.dzengi.com/api/v2");
        assert_eq!(v["api_prefix"], "/api/v2");
        assert_eq!(v["connect_url"], "wss://api-adapter.dzengi.com/connect");
        let dests = v["destinations"].as_array().unwrap();
        assert!(
            dests
                .iter()
                .any(|d| d.as_str() == Some("/api/v2/ticker/24hr"))
        );
    }
}
