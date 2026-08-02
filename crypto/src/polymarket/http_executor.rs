use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use rust_decimal::Decimal;

use crate::error::{CryptoError, Result};

use super::models::*;

const CLOB_BASE: &str = "https://clob.polymarket.com";
const GAMMA_BASE: &str = "https://gamma-api.polymarket.com";

pub struct HttpPolymarketExecutor {
    client: Client,
    api_key: String,
}

impl HttpPolymarketExecutor {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("POLYMARKET_API_KEY").map_err(|_| {
            CryptoError::Config("POLYMARKET_API_KEY not set".into())
        })?;

        Ok(Self {
            client: Client::new(),
            api_key,
        })
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {}", self.api_key)
                .parse()
                .expect("valid header"),
        );
        headers
    }
}

#[async_trait]
impl super::executor::PolymarketExecutor for HttpPolymarketExecutor {
    async fn execute(
        &self,
        signal: &PredictionSignal,
    ) -> Result<PredictionResult> {
        let side_str = match signal.side {
            PredictionSide::Yes => "YES",
            PredictionSide::No => "NO",
        };
        let action_str = match signal.action {
            PredictionOrderAction::Buy => "BUY",
            PredictionOrderAction::Sell => "SELL",
        };

        let body = serde_json::json!({
            "market": signal.market_id,
            "side": side_str,
            "type": action_str,
            "size": signal.shares.to_string(),
            "price": signal.limit_price.map(|p| p.to_string()),
        });

        let resp = self
            .client
            .post(format!("{CLOB_BASE}/order"))
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                CryptoError::Polymarket(format!("Order request failed: {e}"))
            })?;

        let status_code = resp.status();
        let resp_body: serde_json::Value = resp.json().await.map_err(|e| {
            CryptoError::Polymarket(format!("Order response parse failed: {e}"))
        })?;

        if !status_code.is_success() {
            return Err(CryptoError::Polymarket(format!(
                "Polymarket order failed {status_code}: {resp_body}"
            )));
        }

        let order_id = resp_body["orderID"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let filled_shares: Decimal = resp_body["filledSize"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(signal.shares);
        let avg_price: Decimal = resp_body["avgPrice"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(signal.limit_price.unwrap_or(Decimal::new(5, 1)));
        let total_cost = filled_shares * avg_price;

        let order_status =
            match resp_body["status"].as_str().unwrap_or("filled") {
                "filled" | "FILLED" => PredictionStatus::Filled,
                "placed" | "PLACED" | "open" => PredictionStatus::Placed,
                "cancelled" | "CANCELLED" => PredictionStatus::Cancelled,
                _ => PredictionStatus::Failed,
            };

        tracing::info!(
            market = %signal.market_id,
            side = %signal.side,
            action = %signal.action,
            shares = %filled_shares,
            avg_price = %avg_price,
            "Polymarket order executed"
        );

        Ok(PredictionResult {
            order_id,
            market_id: signal.market_id.clone(),
            side: signal.side,
            action: signal.action,
            filled_shares,
            avg_price,
            total_cost,
            status: order_status,
            timestamp: Utc::now(),
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let resp = self
            .client
            .delete(format!("{CLOB_BASE}/order/{order_id}"))
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| {
                CryptoError::Polymarket(format!("Cancel failed: {e}"))
            })?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CryptoError::Polymarket(format!(
                "Cancel order failed: {body}"
            )));
        }

        tracing::info!(order_id, "Polymarket order cancelled");
        Ok(())
    }

    async fn get_market_prices(
        &self,
        market_id: &str,
    ) -> Result<MarketOrderBook> {
        let url = format!("{CLOB_BASE}/book?token_id={market_id}");

        let resp = self.client.get(&url).send().await.map_err(|e| {
            CryptoError::Polymarket(format!("Book fetch failed: {e}"))
        })?;

        let body: serde_json::Value = resp.json().await.map_err(|e| {
            CryptoError::Polymarket(format!("Book parse failed: {e}"))
        })?;

        // The CLOB returns bids/asks arrays; derive mid prices
        let best_bid = extract_best_price(&body["bids"]);
        let best_ask = extract_best_price(&body["asks"]);
        let mid = if best_bid > Decimal::ZERO && best_ask > Decimal::ZERO {
            (best_bid + best_ask) / Decimal::TWO
        } else {
            Decimal::new(5, 1)
        };

        Ok(MarketOrderBook {
            market_id: market_id.to_string(),
            yes_price: mid,
            no_price: Decimal::ONE - mid,
            yes_bid: best_bid,
            yes_ask: best_ask,
            no_bid: Decimal::ONE - best_ask,
            no_ask: Decimal::ONE - best_bid,
            volume_24h: Decimal::ZERO, // not in book endpoint
            liquidity: Decimal::ZERO,
            timestamp: Utc::now(),
        })
    }

    async fn fetch_active_markets(&self) -> Result<Vec<MarketInfo>> {
        let url = format!("{GAMMA_BASE}/markets?closed=false&limit=50");

        let resp = self.client.get(&url).send().await.map_err(|e| {
            CryptoError::Polymarket(format!("Markets fetch failed: {e}"))
        })?;

        let body: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
            CryptoError::Polymarket(format!("Markets parse failed: {e}"))
        })?;

        let markets = body
            .into_iter()
            .filter_map(|m| {
                let market_id = m["condition_id"].as_str()?.to_string();
                let condition_id = market_id.clone();
                let title = m["question"].as_str().unwrap_or("").to_string();
                let description =
                    m["description"].as_str().unwrap_or("").to_string();
                let end_date = m["end_date_iso"]
                    .as_str()
                    .and_then(|s| s.parse::<chrono::DateTime<Utc>>().ok());
                let category = m["category"].as_str().map(String::from);
                let active = m["active"].as_bool().unwrap_or(true);

                Some(MarketInfo {
                    market_id,
                    condition_id,
                    title,
                    description,
                    end_date,
                    category,
                    active,
                })
            })
            .collect();

        Ok(markets)
    }
}

fn extract_best_price(side: &serde_json::Value) -> Decimal {
    side.as_array()
        .and_then(|arr| {
            arr.first().and_then(|entry| {
                entry["price"].as_str().and_then(|s| s.parse().ok())
            })
        })
        .unwrap_or(Decimal::ZERO)
}
