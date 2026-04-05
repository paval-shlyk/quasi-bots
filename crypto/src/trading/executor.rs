use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::error::Result;

use super::models::{TradeResult, TradeSignal, TradeStatus};


#[async_trait]
pub trait TradeExecutor: Send + Sync {
    /// Place an order on the exchange.
    async fn execute(&self, signal: &TradeSignal) -> Result<TradeResult>;

    /// Cancel an open order by exchange-specific ID.
    async fn cancel_order(&self, order_id: &str) -> Result<()>;

    /// Fetch the latest price for a trading pair.
    async fn get_price(&self, pair: &str) -> Result<Decimal>;
}


pub struct PaperTradeExecutor;

#[async_trait]
impl TradeExecutor for PaperTradeExecutor {
    async fn execute(&self, signal: &TradeSignal) -> Result<TradeResult> {
        let fill_price = signal.price.unwrap_or(Decimal::ZERO);
        let fee = signal.quantity * fill_price * Decimal::new(1, 3); // 0.1 %

        tracing::info!(
            pair = %signal.pair,
            side = %signal.side,
            qty = %signal.quantity,
            price = %fill_price,
            "[paper] trade executed"
        );

        Ok(TradeResult {
            order_id: uuid::Uuid::new_v4().to_string(),
            pair: signal.pair.clone(),
            side: signal.side,
            filled_quantity: signal.quantity,
            avg_fill_price: fill_price,
            fee,
            status: TradeStatus::Filled,
            timestamp: chrono::Utc::now(),
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        tracing::info!(order_id, "[paper] order cancelled");
        Ok(())
    }

    async fn get_price(&self, pair: &str) -> Result<Decimal> {
        tracing::debug!(pair, "[paper] returning zero price");
        Ok(Decimal::ZERO)
    }
}
