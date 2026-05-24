use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::error::Result;
use crate::llm::{DecisionEngine, TradeAction, TradingContext};
use crate::settings::BotSettings;

use super::models::*;


#[async_trait]
pub trait TradingStrategy: Send + Sync {
    async fn analyze(
        &self,
        market_data: &MarketData,
        indicators: &TechnicalIndicators,
        settings: &BotSettings,
    ) -> Result<Option<TradeSignal>>;
}


pub struct LlmTradingStrategy {
    decision_engine: Box<dyn DecisionEngine>,
}

impl LlmTradingStrategy {
    pub fn new(decision_engine: Box<dyn DecisionEngine>) -> Self {
        Self { decision_engine }
    }

    /// Post-LLM heuristic filter.  Returns `true` if the signal passes.
    fn passes_heuristics(
        recommendation: &crate::llm::TradeRecommendation,
        indicators: &TechnicalIndicators,
        settings: &BotSettings,
    ) -> bool {
        if recommendation.confidence < settings.confidence_threshold {
            tracing::info!(
                confidence = %recommendation.confidence,
                threshold = %settings.confidence_threshold,
                "Confidence below threshold – filtering"
            );
            return false;
        }

        if let Some(rsi) = indicators.rsi {
            match recommendation.action {
                TradeAction::Buy if rsi > Decimal::new(75, 0) => {
                    tracing::info!(rsi = %rsi, "RSI overbought – filtering buy");
                    return false;
                }
                TradeAction::Sell if rsi < Decimal::new(25, 0) => {
                    tracing::info!(rsi = %rsi, "RSI oversold – filtering sell");
                    return false;
                }
                _ => {}
            }
        }

        if let Some(bb) = &indicators.bollinger {
            if bb.upper - bb.lower == Decimal::ZERO {
                tracing::info!("Zero-width Bollinger bands – filtering");
                return false;
            }
        }

        true
    }
}

#[async_trait]
impl TradingStrategy for LlmTradingStrategy {
    async fn analyze(
        &self,
        market_data: &MarketData,
        indicators: &TechnicalIndicators,
        settings: &BotSettings,
    ) -> Result<Option<TradeSignal>> {
        let context = TradingContext {
            pair: market_data.pair.clone(),
            current_price: market_data.price,
            price_change_24h: if market_data.low_24h != Decimal::ZERO {
                ((market_data.price - market_data.low_24h) / market_data.low_24h)
                    * Decimal::new(100, 0)
            } else {
                Decimal::ZERO
            },
            volume_24h: market_data.volume_24h,
            technical_indicators: serde_json::to_value(indicators)
                .unwrap_or(serde_json::Value::Null),
            portfolio_balance: Decimal::ZERO, // filled by service layer
            open_positions: vec![],
            recent_trades: vec![],
        };

        let rec = self
            .decision_engine
            .get_trade_recommendation(&context)
            .await?;

        if rec.action == TradeAction::Hold {
            return Ok(None);
        }
        if !Self::passes_heuristics(&rec, indicators, settings) {
            return Ok(None);
        }

        let side = match rec.action {
            TradeAction::Buy => TradeSide::Buy,
            TradeAction::Sell => TradeSide::Sell,
            TradeAction::Hold => return Ok(None),
        };

        Ok(Some(TradeSignal {
            pair: market_data.pair.clone(),
            side,
            order_type: OrderType::Limit,
            quantity: Decimal::ZERO, // sized by service based on portfolio
            price: Some(market_data.price),
            stop_loss: None, // set by risk engine in service
            take_profit: None,
            confidence: rec.confidence,
            rationale: rec.rationale,
            timestamp: chrono::Utc::now(),
        }))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::TradeRecommendation;
    use crate::trading::models::BollingerBands;
    use chrono::Utc;

    fn default_settings() -> BotSettings {
        BotSettings::default()
    }

    fn default_indicators() -> TechnicalIndicators {
        TechnicalIndicators {
            rsi: Some(Decimal::new(50, 0)),
            macd: None,
            bollinger: Some(BollingerBands {
                upper: Decimal::new(110, 0),
                middle: Decimal::new(100, 0),
                lower: Decimal::new(90, 0),
            }),
            sma_20: None,
            ema_12: None,
            ema_26: None,
        }
    }

    fn buy_recommendation(confidence: Decimal) -> TradeRecommendation {
        TradeRecommendation {
            action: TradeAction::Buy,
            pair: "BTC/USDC".into(),
            size_percent: Decimal::new(10, 0),
            confidence,
            rationale: "test".into(),
            timestamp: Utc::now(),
        }
    }

    fn sell_recommendation(confidence: Decimal) -> TradeRecommendation {
        TradeRecommendation {
            action: TradeAction::Sell,
            pair: "BTC/USDC".into(),
            size_percent: Decimal::new(10, 0),
            confidence,
            rationale: "test".into(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn passes_with_normal_indicators_and_high_confidence() {
        let rec = buy_recommendation(Decimal::new(85, 2)); // 0.85
        let indicators = default_indicators();
        let settings = default_settings();
        assert!(LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn rejects_low_confidence() {
        let rec = buy_recommendation(Decimal::new(3, 1)); // 0.3, below 0.7 threshold
        let indicators = default_indicators();
        let settings = default_settings();
        assert!(!LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn rejects_buy_when_rsi_overbought() {
        let rec = buy_recommendation(Decimal::new(9, 1)); // 0.9
        let mut indicators = default_indicators();
        indicators.rsi = Some(Decimal::new(80, 0)); // overbought (> 75)
        let settings = default_settings();
        assert!(!LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn allows_sell_when_rsi_overbought() {
        let rec = sell_recommendation(Decimal::new(9, 1));
        let mut indicators = default_indicators();
        indicators.rsi = Some(Decimal::new(80, 0));
        let settings = default_settings();
        assert!(LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn rejects_sell_when_rsi_oversold() {
        let rec = sell_recommendation(Decimal::new(9, 1));
        let mut indicators = default_indicators();
        indicators.rsi = Some(Decimal::new(20, 0)); // oversold (< 25)
        let settings = default_settings();
        assert!(!LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn allows_buy_when_rsi_oversold() {
        let rec = buy_recommendation(Decimal::new(9, 1));
        let mut indicators = default_indicators();
        indicators.rsi = Some(Decimal::new(20, 0));
        let settings = default_settings();
        assert!(LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn rejects_zero_width_bollinger() {
        let rec = buy_recommendation(Decimal::new(9, 1));
        let mut indicators = default_indicators();
        indicators.bollinger = Some(BollingerBands {
            upper: Decimal::new(100, 0),
            middle: Decimal::new(100, 0),
            lower: Decimal::new(100, 0),
        });
        let settings = default_settings();
        assert!(!LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn passes_without_optional_indicators() {
        let rec = buy_recommendation(Decimal::new(85, 2));
        let indicators = TechnicalIndicators {
            rsi: None,
            macd: None,
            bollinger: None,
            sma_20: None,
            ema_12: None,
            ema_26: None,
        };
        let settings = default_settings();
        assert!(LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn rsi_at_boundary_75_allows_buy() {
        // RSI exactly at 75 should pass (we filter > 75, not >=)
        let rec = buy_recommendation(Decimal::new(9, 1));
        let mut indicators = default_indicators();
        indicators.rsi = Some(Decimal::new(75, 0));
        let settings = default_settings();
        assert!(LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }

    #[test]
    fn rsi_at_boundary_25_allows_sell() {
        // RSI exactly at 25 should pass (we filter < 25, not <=)
        let rec = sell_recommendation(Decimal::new(9, 1));
        let mut indicators = default_indicators();
        indicators.rsi = Some(Decimal::new(25, 0));
        let settings = default_settings();
        assert!(LlmTradingStrategy::passes_heuristics(&rec, &indicators, &settings));
    }
}
