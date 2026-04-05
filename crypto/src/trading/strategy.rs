use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::error::Result;
use crate::llm::{DecisionEngine, TradeAction, TradingContext};
use crate::settings::BotSettings;

use super::models::*;

// ---------------------------------------------------------------------------
// Strategy trait
// ---------------------------------------------------------------------------

/// Analyses market data + indicators and optionally emits a [`TradeSignal`].
#[async_trait]
pub trait TradingStrategy: Send + Sync {
    async fn analyze(
        &self,
        market_data: &MarketData,
        indicators: &TechnicalIndicators,
        settings: &BotSettings,
    ) -> Result<Option<TradeSignal>>;
}

// ---------------------------------------------------------------------------
// Default LLM + heuristic strategy
// ---------------------------------------------------------------------------

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
        // 1. Confidence gate
        if recommendation.confidence < settings.confidence_threshold {
            tracing::info!(
                confidence = %recommendation.confidence,
                threshold = %settings.confidence_threshold,
                "Confidence below threshold – filtering"
            );
            return false;
        }

        // 2. RSI filter: avoid buying overbought / selling oversold
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

        // 3. Bollinger band-width sanity check
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
