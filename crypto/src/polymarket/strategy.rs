use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::error::Result;
use crate::llm::{DecisionEngine, PredictionAction, PredictionContext};
use crate::settings::BotSettings;

use super::models::*;


#[async_trait]
pub trait PolymarketStrategy: Send + Sync {
    async fn analyze(
        &self,
        market: &MarketInfo,
        order_book: &MarketOrderBook,
        settings: &BotSettings,
    ) -> Result<Option<PredictionSignal>>;
}


pub struct LlmPolymarketStrategy {
    decision_engine: Box<dyn DecisionEngine>,
}

impl LlmPolymarketStrategy {
    pub fn new(decision_engine: Box<dyn DecisionEngine>) -> Self {
        Self { decision_engine }
    }

    fn calculate_edge(
        llm_confidence: Decimal,
        action: &PredictionAction,
        order_book: &MarketOrderBook,
    ) -> MarketEdge {
        let (market_prob, side) = match action {
            PredictionAction::BuyYes => (order_book.yes_price, PredictionSide::Yes),
            PredictionAction::BuyNo => (order_book.no_price, PredictionSide::No),
            _ => (order_book.yes_price, PredictionSide::Yes),
        };
        MarketEdge {
            market_id: order_book.market_id.clone(),
            llm_probability: llm_confidence,
            market_implied_probability: market_prob,
            edge: llm_confidence - market_prob,
            side,
        }
    }

    fn passes_heuristics(
        edge: &MarketEdge,
        order_book: &MarketOrderBook,
        settings: &BotSettings,
    ) -> bool {
        let min_edge = Decimal::new(5, 2);
        if edge.edge.abs() < min_edge {
            tracing::info!(
                market = %edge.market_id,
                edge = %edge.edge,
                "Edge below minimum threshold"
            );
            return false;
        }

        if order_book.liquidity < settings.min_liquidity_threshold {
            tracing::info!(
                market = %edge.market_id,
                liquidity = %order_book.liquidity,
                threshold = %settings.min_liquidity_threshold,
                "Insufficient liquidity"
            );
            return false;
        }

        let low = Decimal::new(5, 2);
        let high = Decimal::new(95, 2);
        if order_book.yes_price < low || order_book.yes_price > high {
            tracing::info!(
                market = %edge.market_id,
                yes_price = %order_book.yes_price,
                "Price at extreme – skipping"
            );
            return false;
        }

        true
    }
}

#[async_trait]
impl PolymarketStrategy for LlmPolymarketStrategy {
    async fn analyze(
        &self,
        market: &MarketInfo,
        order_book: &MarketOrderBook,
        settings: &BotSettings,
    ) -> Result<Option<PredictionSignal>> {
        if !market.active {
            return Ok(None);
        }

        let context = PredictionContext {
            market_id: market.market_id.clone(),
            market_title: market.title.clone(),
            description: market.description.clone(),
            current_yes_price: order_book.yes_price,
            current_no_price: order_book.no_price,
            volume: order_book.volume_24h,
            liquidity: order_book.liquidity,
            end_date: market.end_date,
            portfolio_balance: Decimal::ZERO, // filled by service
            open_predictions: vec![],
        };

        let rec = self
            .decision_engine
            .get_prediction_recommendation(&context)
            .await?;

        if rec.action == PredictionAction::Hold {
            return Ok(None);
        }
        if rec.confidence < settings.confidence_threshold {
            return Ok(None);
        }

        let edge = Self::calculate_edge(rec.confidence, &rec.action, order_book);
        if !Self::passes_heuristics(&edge, order_book, settings) {
            return Ok(None);
        }

        let (side, action, limit_price) = match rec.action {
            PredictionAction::BuyYes => {
                (PredictionSide::Yes, PredictionOrderAction::Buy, order_book.yes_ask)
            }
            PredictionAction::BuyNo => {
                (PredictionSide::No, PredictionOrderAction::Buy, order_book.no_ask)
            }
            PredictionAction::Sell => {
                (PredictionSide::Yes, PredictionOrderAction::Sell, order_book.yes_bid)
            }
            PredictionAction::Hold => return Ok(None),
        };

        let shares = if limit_price > Decimal::ZERO {
            rec.size_usdc / limit_price
        } else {
            Decimal::ZERO
        };

        Ok(Some(PredictionSignal {
            market_id: market.market_id.clone(),
            market_title: market.title.clone(),
            side,
            action,
            shares,
            limit_price: Some(limit_price),
            confidence: rec.confidence,
            rationale: rec.rationale,
            timestamp: chrono::Utc::now(),
        }))
    }
}
