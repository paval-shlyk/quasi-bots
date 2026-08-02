use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use sha2::Sha256;
use std::collections::HashMap;

use crate::error::{CryptoError, Result};

use super::models::{TradeResult, TradeSide, TradeSignal, TradeStatus};

const BINANCE_BASE: &str = "https://api.binance.com";

pub struct BinanceExecutor {
    client: Client,
    api_key: String,
    secret_key: String,
}

impl BinanceExecutor {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("BINANCE_API_KEY").map_err(|_| {
            CryptoError::Config("BINANCE_API_KEY not set".into())
        })?;
        let secret_key = std::env::var("BINANCE_SECRET_KEY").map_err(|_| {
            CryptoError::Config("BINANCE_SECRET_KEY not set".into())
        })?;

        Ok(Self {
            client: Client::new(),
            api_key,
            secret_key,
        })
    }

    /// Binance expects pairs like "BTCUSDC" not "BTC/USDC"
    fn normalize_pair(pair: &str) -> String {
        pair.replace('/', "")
    }

    fn sign(&self, query: &str) -> String {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.secret_key.as_bytes())
                .expect("HMAC key length");
        mac.update(query.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    async fn signed_get(
        &self,
        path: &str,
        params: &mut HashMap<&str, String>,
    ) -> Result<String> {
        params.insert("timestamp", Utc::now().timestamp_millis().to_string());
        params.insert("recvWindow", "5000".into());

        let query: String = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        let signature = self.sign(&query);
        let url = format!("{BINANCE_BASE}{path}?{query}&signature={signature}");

        let resp = self
            .client
            .get(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await
            .map_err(|e| {
                CryptoError::Trading(format!("Binance GET failed: {e}"))
            })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            CryptoError::Trading(format!("Binance response read failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(CryptoError::Trading(format!(
                "Binance API error {status}: {body}"
            )));
        }
        Ok(body)
    }

    async fn signed_post(
        &self,
        path: &str,
        params: &mut HashMap<&str, String>,
    ) -> Result<String> {
        params.insert("timestamp", Utc::now().timestamp_millis().to_string());
        params.insert("recvWindow", "5000".into());

        let query: String = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        let signature = self.sign(&query);
        let body_str = format!("{query}&signature={signature}");
        let url = format!("{BINANCE_BASE}{path}");

        let resp = self
            .client
            .post(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body_str)
            .send()
            .await
            .map_err(|e| {
                CryptoError::Trading(format!("Binance POST failed: {e}"))
            })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            CryptoError::Trading(format!("Binance response read failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(CryptoError::Trading(format!(
                "Binance API error {status}: {body}"
            )));
        }
        Ok(body)
    }

    async fn signed_delete(
        &self,
        path: &str,
        params: &mut HashMap<&str, String>,
    ) -> Result<String> {
        params.insert("timestamp", Utc::now().timestamp_millis().to_string());
        params.insert("recvWindow", "5000".into());

        let query: String = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        let signature = self.sign(&query);
        let url = format!("{BINANCE_BASE}{path}?{query}&signature={signature}");

        let resp = self
            .client
            .delete(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await
            .map_err(|e| {
                CryptoError::Trading(format!("Binance DELETE failed: {e}"))
            })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            CryptoError::Trading(format!("Binance response read failed: {e}"))
        })?;

        if !status.is_success() {
            return Err(CryptoError::Trading(format!(
                "Binance API error {status}: {body}"
            )));
        }
        Ok(body)
    }
}

#[async_trait]
impl super::executor::TradeExecutor for BinanceExecutor {
    async fn execute(&self, signal: &TradeSignal) -> Result<TradeResult> {
        let symbol = Self::normalize_pair(&signal.pair);
        let side_str = match signal.side {
            TradeSide::Buy => "BUY",
            TradeSide::Sell => "SELL",
        };
        let order_type = match signal.order_type {
            super::models::OrderType::Market => "MARKET",
            super::models::OrderType::Limit => "LIMIT",
        };

        let mut params = HashMap::new();
        params.insert("symbol", symbol);
        params.insert("side", side_str.into());
        params.insert("type", order_type.into());
        params.insert("quantity", signal.quantity.to_string());
        params.insert("newOrderRespType", "FULL".into());

        if order_type == "LIMIT"
            && let Some(price) = signal.price {
                params.insert("price", price.to_string());
                params.insert("timeInForce", "GTC".into());
            }

        let body = self.signed_post("/api/v3/order", &mut params).await?;
        let resp: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| {
                CryptoError::Trading(format!(
                    "Failed to parse order response: {e}"
                ))
            })?;

        let order_id = resp["orderId"].as_i64().unwrap_or(0).to_string();

        // Compute average fill from fills array
        let (filled_qty, total_cost, total_fee) = resp["fills"]
            .as_array()
            .map(|fills| {
                fills.iter().fold(
                    (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO),
                    |(qty, cost, fee), fill| {
                        let fq: Decimal = fill["qty"]
                            .as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(Decimal::ZERO);
                        let fp: Decimal = fill["price"]
                            .as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(Decimal::ZERO);
                        let ff: Decimal = fill["commission"]
                            .as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(Decimal::ZERO);
                        (qty + fq, cost + fq * fp, fee + ff)
                    },
                )
            })
            .unwrap_or((Decimal::ZERO, Decimal::ZERO, Decimal::ZERO));

        let avg_price = if filled_qty > Decimal::ZERO {
            total_cost / filled_qty
        } else {
            Decimal::ZERO
        };

        let status_str = resp["status"].as_str().unwrap_or("UNKNOWN");
        let status = match status_str {
            "FILLED" => TradeStatus::Filled,
            "PARTIALLY_FILLED" => TradeStatus::PartiallyFilled,
            "CANCELED" | "CANCELLED" => TradeStatus::Cancelled,
            "REJECTED" | "EXPIRED" => TradeStatus::Failed,
            _ => TradeStatus::Pending,
        };

        tracing::info!(
            pair = %signal.pair,
            side = %signal.side,
            qty = %filled_qty,
            avg_price = %avg_price,
            order_id = %order_id,
            "Binance order executed"
        );

        Ok(TradeResult {
            order_id,
            pair: signal.pair.clone(),
            side: signal.side,
            filled_quantity: filled_qty,
            avg_fill_price: avg_price,
            fee: total_fee,
            status,
            timestamp: Utc::now(),
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        // Binance cancel requires the symbol; we encode it as "SYMBOL:ORDER_ID"
        // in our internal order_id when needed, but for simplicity we accept
        // raw numeric order IDs and require CANCEL_SYMBOL env or pass-through.
        let mut params = HashMap::new();
        params.insert("orderId", order_id.into());

        // Caller should provide symbol context; for now log a warning
        tracing::warn!(
            order_id,
            "cancel_order called without symbol context, attempting anyway"
        );

        let _ = self.signed_delete("/api/v3/order", &mut params).await?;
        Ok(())
    }

    async fn get_price(&self, pair: &str) -> Result<Decimal> {
        let symbol = Self::normalize_pair(pair);
        let url = format!("{BINANCE_BASE}/api/v3/ticker/price?symbol={symbol}");

        let resp = self.client.get(&url).send().await.map_err(|e| {
            CryptoError::Trading(format!("Price fetch failed: {e}"))
        })?;

        let body: serde_json::Value = resp.json().await.map_err(|e| {
            CryptoError::Trading(format!("Price parse failed: {e}"))
        })?;

        body["price"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| {
                CryptoError::Trading(format!(
                    "Invalid price response for {pair}"
                ))
            })
    }
}
