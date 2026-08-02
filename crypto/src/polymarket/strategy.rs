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
            PredictionAction::BuyYes => {
                (order_book.yes_price, PredictionSide::Yes)
            }
            PredictionAction::BuyNo => {
                (order_book.no_price, PredictionSide::No)
            }
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

        let edge =
            Self::calculate_edge(rec.confidence, &rec.action, order_book);
        if !Self::passes_heuristics(&edge, order_book, settings) {
            return Ok(None);
        }

        let (side, action, limit_price) = match rec.action {
            PredictionAction::BuyYes => (
                PredictionSide::Yes,
                PredictionOrderAction::Buy,
                order_book.yes_ask,
            ),
            PredictionAction::BuyNo => (
                PredictionSide::No,
                PredictionOrderAction::Buy,
                order_book.no_ask,
            ),
            PredictionAction::Sell => (
                PredictionSide::Yes,
                PredictionOrderAction::Sell,
                order_book.yes_bid,
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::PredictionAction;

    fn default_settings() -> BotSettings {
        BotSettings::default()
    }

    fn default_order_book() -> MarketOrderBook {
        MarketOrderBook {
            market_id: "mkt-1".into(),
            yes_price: Decimal::new(60, 2), // 0.60
            no_price: Decimal::new(40, 2),  // 0.40
            yes_bid: Decimal::new(59, 2),
            yes_ask: Decimal::new(61, 2),
            no_bid: Decimal::new(39, 2),
            no_ask: Decimal::new(41, 2),
            volume_24h: Decimal::new(100000, 0),
            liquidity: Decimal::new(200, 0), // well above default threshold (50)
            timestamp: chrono::Utc::now(),
        }
    }

    #[test]
    fn calculate_edge_buy_yes() {
        let ob = default_order_book();
        // LLM confidence 0.80 vs market yes price 0.60 -> edge 0.20
        let edge = LlmPolymarketStrategy::calculate_edge(
            Decimal::new(80, 2),
            &PredictionAction::BuyYes,
            &ob,
        );
        assert_eq!(edge.edge, Decimal::new(20, 2));
        assert_eq!(edge.side, PredictionSide::Yes);
        assert_eq!(edge.market_implied_probability, Decimal::new(60, 2));
    }

    #[test]
    fn calculate_edge_buy_no() {
        let ob = default_order_book();
        // LLM confidence 0.70 vs market no price 0.40 -> edge 0.30
        let edge = LlmPolymarketStrategy::calculate_edge(
            Decimal::new(70, 2),
            &PredictionAction::BuyNo,
            &ob,
        );
        assert_eq!(edge.edge, Decimal::new(30, 2));
        assert_eq!(edge.side, PredictionSide::No);
    }

    #[test]
    fn heuristics_pass_with_adequate_edge_and_liquidity() {
        let ob = default_order_book();
        let edge = MarketEdge {
            market_id: "mkt-1".into(),
            llm_probability: Decimal::new(80, 2),
            market_implied_probability: Decimal::new(60, 2),
            edge: Decimal::new(20, 2), // 0.20, well above min 0.05
            side: PredictionSide::Yes,
        };
        let settings = default_settings();
        assert!(LlmPolymarketStrategy::passes_heuristics(
            &edge, &ob, &settings
        ));
    }

    #[test]
    fn heuristics_reject_small_edge() {
        let ob = default_order_book();
        let edge = MarketEdge {
            market_id: "mkt-1".into(),
            llm_probability: Decimal::new(62, 2),
            market_implied_probability: Decimal::new(60, 2),
            edge: Decimal::new(2, 2), // 0.02, below min 0.05
            side: PredictionSide::Yes,
        };
        let settings = default_settings();
        assert!(!LlmPolymarketStrategy::passes_heuristics(
            &edge, &ob, &settings
        ));
    }

    #[test]
    fn heuristics_reject_low_liquidity() {
        let mut ob = default_order_book();
        ob.liquidity = Decimal::new(10, 0); // below default threshold of 50
        let edge = MarketEdge {
            market_id: "mkt-1".into(),
            llm_probability: Decimal::new(80, 2),
            market_implied_probability: Decimal::new(60, 2),
            edge: Decimal::new(20, 2),
            side: PredictionSide::Yes,
        };
        let settings = default_settings();
        assert!(!LlmPolymarketStrategy::passes_heuristics(
            &edge, &ob, &settings
        ));
    }

    #[test]
    fn heuristics_reject_extreme_price_low() {
        let mut ob = default_order_book();
        ob.yes_price = Decimal::new(3, 2); // 0.03, below 0.05
        let edge = MarketEdge {
            market_id: "mkt-1".into(),
            llm_probability: Decimal::new(80, 2),
            market_implied_probability: Decimal::new(3, 2),
            edge: Decimal::new(77, 2),
            side: PredictionSide::Yes,
        };
        let settings = default_settings();
        assert!(!LlmPolymarketStrategy::passes_heuristics(
            &edge, &ob, &settings
        ));
    }

    #[test]
    fn heuristics_reject_extreme_price_high() {
        let mut ob = default_order_book();
        ob.yes_price = Decimal::new(97, 2); // 0.97, above 0.95
        let edge = MarketEdge {
            market_id: "mkt-1".into(),
            llm_probability: Decimal::new(99, 2),
            market_implied_probability: Decimal::new(97, 2),
            edge: Decimal::new(2, 2),
            side: PredictionSide::Yes,
        };
        let settings = default_settings();
        assert!(!LlmPolymarketStrategy::passes_heuristics(
            &edge, &ob, &settings
        ));
    }

    #[test]
    fn negative_edge_still_rejected_if_too_small() {
        let ob = default_order_book();
        // Negative edge with abs < 0.05
        let edge = MarketEdge {
            market_id: "mkt-1".into(),
            llm_probability: Decimal::new(58, 2),
            market_implied_probability: Decimal::new(60, 2),
            edge: Decimal::new(-2, 2), // -0.02
            side: PredictionSide::Yes,
        };
        let settings = default_settings();
        assert!(!LlmPolymarketStrategy::passes_heuristics(
            &edge, &ob, &settings
        ));
    }
}
