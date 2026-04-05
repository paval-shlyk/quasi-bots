use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::error::Result;

use super::models::*;


#[async_trait]
pub trait PolymarketExecutor: Send + Sync {
    /// Place an order on the Polymarket CLOB.
    async fn execute(&self, signal: &PredictionSignal) -> Result<PredictionResult>;

    /// Cancel an open order.
    async fn cancel_order(&self, order_id: &str) -> Result<()>;

    /// Current order-book for a given market.
    async fn get_market_prices(&self, market_id: &str) -> Result<MarketOrderBook>;

    /// Fetch the list of active prediction markets.
    async fn fetch_active_markets(&self) -> Result<Vec<MarketInfo>>;
}


pub struct PaperPolymarketExecutor;

#[async_trait]
impl PolymarketExecutor for PaperPolymarketExecutor {
    async fn execute(&self, signal: &PredictionSignal) -> Result<PredictionResult> {
        let price = signal.limit_price.unwrap_or(Decimal::new(5, 1));
        let cost = signal.shares * price;

        tracing::info!(
            market = %signal.market_id,
            side = %signal.side,
            action = %signal.action,
            shares = %signal.shares,
            price = %price,
            "[paper] prediction executed"
        );

        Ok(PredictionResult {
            order_id: uuid::Uuid::new_v4().to_string(),
            market_id: signal.market_id.clone(),
            side: signal.side,
            action: signal.action,
            filled_shares: signal.shares,
            avg_price: price,
            total_cost: cost,
            status: PredictionStatus::Filled,
            timestamp: chrono::Utc::now(),
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        tracing::info!(order_id, "[paper] prediction order cancelled");
        Ok(())
    }

    async fn get_market_prices(&self, market_id: &str) -> Result<MarketOrderBook> {
        Ok(MarketOrderBook {
            market_id: market_id.to_string(),
            yes_price: Decimal::new(5, 1),
            no_price: Decimal::new(5, 1),
            yes_bid: Decimal::new(49, 2),
            yes_ask: Decimal::new(51, 2),
            no_bid: Decimal::new(49, 2),
            no_ask: Decimal::new(51, 2),
            volume_24h: Decimal::new(10000, 0),
            liquidity: Decimal::new(5000, 0),
            timestamp: chrono::Utc::now(),
        })
    }

    async fn fetch_active_markets(&self) -> Result<Vec<MarketInfo>> {
        Ok(vec![])
    }
}
