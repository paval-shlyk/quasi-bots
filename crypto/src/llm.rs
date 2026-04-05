use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::Result;


/// Which LLM backend this bot instance uses (selected at startup via config).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LlmProvider {
    Grok,
    Gemini,
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TradeAction {
    Buy,
    Sell,
    Hold,
}

/// Structured recommendation the LLM returns for crypto trading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecommendation {
    pub action: TradeAction,
    pub pair: String,
    pub size_percent: Decimal,
    pub confidence: Decimal,
    pub rationale: String,
    pub timestamp: DateTime<Utc>,
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PredictionAction {
    BuyYes,
    BuyNo,
    Sell,
    Hold,
}

/// Structured recommendation the LLM returns for prediction markets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionRecommendation {
    pub action: PredictionAction,
    pub market_id: String,
    pub size_usdc: Decimal,
    pub confidence: Decimal,
    pub rationale: String,
    pub timestamp: DateTime<Utc>,
}


/// Market + portfolio context assembled before calling the LLM for a trade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingContext {
    pub pair: String,
    pub current_price: Decimal,
    pub price_change_24h: Decimal,
    pub volume_24h: Decimal,
    pub technical_indicators: serde_json::Value,
    pub portfolio_balance: Decimal,
    pub open_positions: Vec<serde_json::Value>,
    pub recent_trades: Vec<serde_json::Value>,
}

/// Market + portfolio context assembled before calling the LLM for a prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionContext {
    pub market_id: String,
    pub market_title: String,
    pub description: String,
    pub current_yes_price: Decimal,
    pub current_no_price: Decimal,
    pub volume: Decimal,
    pub liquidity: Decimal,
    pub end_date: Option<DateTime<Utc>>,
    pub portfolio_balance: Decimal,
    pub open_predictions: Vec<serde_json::Value>,
}


/// Abstraction over LLM providers.
/// In production each instance wires either xAI/Grok or Gemini behind this
/// trait via `rig-core`.  A [`HeuristicFallback`] implementation is provided
/// for when the LLM is unavailable.
#[async_trait]
pub trait DecisionEngine: Send + Sync {
    async fn get_trade_recommendation(
        &self,
        context: &TradingContext,
    ) -> Result<TradeRecommendation>;

    async fn get_prediction_recommendation(
        &self,
        context: &PredictionContext,
    ) -> Result<PredictionRecommendation>;

    fn provider(&self) -> LlmProvider;
}


/// Used when the LLM is unreachable (rate-limited, network error, etc.).
pub struct HeuristicFallback;

#[async_trait]
impl DecisionEngine for HeuristicFallback {
    async fn get_trade_recommendation(
        &self,
        context: &TradingContext,
    ) -> Result<TradeRecommendation> {
        Ok(TradeRecommendation {
            action: TradeAction::Hold,
            pair: context.pair.clone(),
            size_percent: Decimal::ZERO,
            confidence: Decimal::new(5, 1), // 0.5
            rationale: "Heuristic fallback: no LLM available, holding position".into(),
            timestamp: Utc::now(),
        })
    }

    async fn get_prediction_recommendation(
        &self,
        context: &PredictionContext,
    ) -> Result<PredictionRecommendation> {
        Ok(PredictionRecommendation {
            action: PredictionAction::Hold,
            market_id: context.market_id.clone(),
            size_usdc: Decimal::ZERO,
            confidence: Decimal::new(5, 1),
            rationale: "Heuristic fallback: no LLM available, holding".into(),
            timestamp: Utc::now(),
        })
    }

    fn provider(&self) -> LlmProvider {
        LlmProvider::Grok // irrelevant for fallback
    }
}
