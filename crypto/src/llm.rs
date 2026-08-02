use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rig::completion::Prompt;
use rig::prelude::CompletionClient;
use rig::providers::openai;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{CryptoError, Result};

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

// ---------------------------------------------------------------------------
// Rig-based LLM decision engine
// ---------------------------------------------------------------------------

const TRADING_SYSTEM_PROMPT: &str = "\
You are a crypto trading analyst. You receive market data and technical indicators \
for a trading pair, then decide whether to BUY, SELL, or HOLD.\n\n\
Respond ONLY with a JSON object (no markdown, no explanation outside the JSON):\n\
{\n  \"action\": \"Buy\" | \"Sell\" | \"Hold\",\n  \"size_percent\": <number 0-100>,\n  \
\"confidence\": <number 0.0-1.0>,\n  \"rationale\": \"<one sentence>\"\n}\n\n\
Rules:\n\
- confidence must reflect how sure you are (0.0 = unsure, 1.0 = certain)\n\
- size_percent is the fraction of available budget to allocate (0 for Hold)\n\
- Be conservative: prefer Hold when indicators are mixed\n\
- Factor in RSI, MACD, Bollinger bands, volume, and price momentum";

const PREDICTION_SYSTEM_PROMPT: &str = "\
You are a prediction market analyst. You receive market info and order book data \
for a Polymarket event, then decide whether to BUY YES, BUY NO, SELL, or HOLD.\n\n\
Respond ONLY with a JSON object (no markdown, no explanation outside the JSON):\n\
{\n  \"action\": \"BuyYes\" | \"BuyNo\" | \"Sell\" | \"Hold\",\n  \"size_usdc\": <number>,\n  \
\"confidence\": <number 0.0-1.0>,\n  \"rationale\": \"<one sentence>\"\n}\n\n\
Rules:\n\
- confidence represents your estimated true probability of the YES outcome\n\
- Only BuyYes if confidence > market_yes_price (you think market underprices YES)\n\
- Only BuyNo if (1 - confidence) > market_no_price\n\
- size_usdc is in USDC, be conservative with sizing\n\
- Prefer Hold when the edge is small or data is ambiguous";

/// LLM-backed decision engine using rig-core. Supports xAI (Grok) and
/// Google Gemini. The agent is initialized once at startup with a system
/// prompt; each call sends the serialized context as a user message.
pub struct RigDecisionEngine {
    provider: LlmProvider,
    trading_agent: rig::agent::Agent<openai::completion::CompletionModel>,
    prediction_agent: rig::agent::Agent<openai::completion::CompletionModel>,
}

impl RigDecisionEngine {
    /// Both xAI and Gemini expose OpenAI-compatible chat completions
    /// endpoints, so we use the CompletionsClient with a custom base URL.
    pub fn from_env(
        llm_provider: LlmProvider,
        temperature: f64,
    ) -> Result<Self> {
        let (api_key_var, base_url, model) = match llm_provider {
            LlmProvider::Grok => {
                ("XAI_API_KEY", "https://api.x.ai/v1", "grok-3-mini")
            }
            LlmProvider::Gemini => (
                "GEMINI_API_KEY",
                "https://generativelanguage.googleapis.com/v1beta/openai",
                "gemini-2.0-flash",
            ),
        };

        let api_key = std::env::var(api_key_var).map_err(|_| {
            CryptoError::Config(format!("{api_key_var} not set"))
        })?;

        let client = openai::CompletionsClient::builder()
            .api_key(&api_key)
            .base_url(base_url)
            .build()
            .map_err(|e| {
                CryptoError::Config(format!("Failed to build LLM client: {e}"))
            })?;

        let trading_agent = client
            .agent(model)
            .preamble(TRADING_SYSTEM_PROMPT)
            .temperature(temperature)
            .build();

        let prediction_agent = client
            .agent(model)
            .preamble(PREDICTION_SYSTEM_PROMPT)
            .temperature(temperature)
            .build();

        Ok(Self {
            provider: llm_provider,
            trading_agent,
            prediction_agent,
        })
    }
}

#[async_trait]
impl DecisionEngine for RigDecisionEngine {
    async fn get_trade_recommendation(
        &self,
        context: &TradingContext,
    ) -> Result<TradeRecommendation> {
        let context_json =
            serde_json::to_string_pretty(context).map_err(|e| {
                CryptoError::Llm(format!("Failed to serialize context: {e}"))
            })?;

        let raw_response = self
            .trading_agent
            .prompt(&context_json)
            .await
            .map_err(|e| {
                CryptoError::Llm(format!("LLM trading prompt failed: {e}"))
            })?;

        parse_trade_response(&raw_response, &context.pair)
    }

    async fn get_prediction_recommendation(
        &self,
        context: &PredictionContext,
    ) -> Result<PredictionRecommendation> {
        let context_json =
            serde_json::to_string_pretty(context).map_err(|e| {
                CryptoError::Llm(format!("Failed to serialize context: {e}"))
            })?;

        let raw_response = self
            .prediction_agent
            .prompt(&context_json)
            .await
            .map_err(|e| {
                CryptoError::Llm(format!("LLM prediction prompt failed: {e}"))
            })?;

        parse_prediction_response(&raw_response, &context.market_id)
    }

    fn provider(&self) -> LlmProvider {
        self.provider.clone()
    }
}

/// Strip markdown fences if the LLM wraps its response in ```json ... ```.
fn strip_json_fences(s: &str) -> &str {
    let trimmed = s.trim();
    let inner = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    inner.strip_suffix("```").unwrap_or(inner).trim()
}

fn parse_trade_response(raw: &str, pair: &str) -> Result<TradeRecommendation> {
    let cleaned = strip_json_fences(raw);
    let v: serde_json::Value = serde_json::from_str(cleaned).map_err(|e| {
        CryptoError::Llm(format!("LLM returned invalid JSON: {e}\nRaw: {raw}"))
    })?;

    let action = match v["action"].as_str().unwrap_or("Hold") {
        "Buy" | "buy" | "BUY" => TradeAction::Buy,
        "Sell" | "sell" | "SELL" => TradeAction::Sell,
        _ => TradeAction::Hold,
    };

    let size_percent = v["size_percent"]
        .as_f64()
        .and_then(|f| Decimal::try_from(f).ok())
        .unwrap_or(Decimal::ZERO);

    let confidence = v["confidence"]
        .as_f64()
        .and_then(|f| Decimal::try_from(f).ok())
        .unwrap_or(Decimal::new(5, 1));

    let rationale = v["rationale"]
        .as_str()
        .unwrap_or("No rationale provided")
        .to_string();

    Ok(TradeRecommendation {
        action,
        pair: pair.to_string(),
        size_percent,
        confidence,
        rationale,
        timestamp: Utc::now(),
    })
}

fn parse_prediction_response(
    raw: &str,
    market_id: &str,
) -> Result<PredictionRecommendation> {
    let cleaned = strip_json_fences(raw);
    let v: serde_json::Value = serde_json::from_str(cleaned).map_err(|e| {
        CryptoError::Llm(format!("LLM returned invalid JSON: {e}\nRaw: {raw}"))
    })?;

    let action = match v["action"].as_str().unwrap_or("Hold") {
        "BuyYes" | "buyYes" | "buy_yes" => PredictionAction::BuyYes,
        "BuyNo" | "buyNo" | "buy_no" => PredictionAction::BuyNo,
        "Sell" | "sell" | "SELL" => PredictionAction::Sell,
        _ => PredictionAction::Hold,
    };

    let size_usdc = v["size_usdc"]
        .as_f64()
        .and_then(|f| Decimal::try_from(f).ok())
        .unwrap_or(Decimal::ZERO);

    let confidence = v["confidence"]
        .as_f64()
        .and_then(|f| Decimal::try_from(f).ok())
        .unwrap_or(Decimal::new(5, 1));

    let rationale = v["rationale"]
        .as_str()
        .unwrap_or("No rationale provided")
        .to_string();

    Ok(PredictionRecommendation {
        action,
        market_id: market_id.to_string(),
        size_usdc,
        confidence,
        rationale,
        timestamp: Utc::now(),
    })
}

// ---------------------------------------------------------------------------
// Fallback wrapper: try real LLM, fall back to heuristics on error
// ---------------------------------------------------------------------------

/// Wraps a primary [`DecisionEngine`] (typically [`RigDecisionEngine`]) and
/// falls back to [`HeuristicFallback`] when the primary returns an error
/// (network timeout, rate limit, malformed response, etc.).
pub struct FallbackDecisionEngine {
    primary: Box<dyn DecisionEngine>,
    fallback: HeuristicFallback,
}

impl FallbackDecisionEngine {
    pub fn new(primary: Box<dyn DecisionEngine>) -> Self {
        Self {
            primary,
            fallback: HeuristicFallback,
        }
    }
}

#[async_trait]
impl DecisionEngine for FallbackDecisionEngine {
    async fn get_trade_recommendation(
        &self,
        context: &TradingContext,
    ) -> Result<TradeRecommendation> {
        match self.primary.get_trade_recommendation(context).await {
            Ok(rec) => Ok(rec),
            Err(e) => {
                tracing::warn!(error = %e, pair = %context.pair, "LLM trade call failed, using heuristic fallback");
                self.fallback.get_trade_recommendation(context).await
            }
        }
    }

    async fn get_prediction_recommendation(
        &self,
        context: &PredictionContext,
    ) -> Result<PredictionRecommendation> {
        match self.primary.get_prediction_recommendation(context).await {
            Ok(rec) => Ok(rec),
            Err(e) => {
                tracing::warn!(error = %e, market = %context.market_id, "LLM prediction call failed, using heuristic fallback");
                self.fallback.get_prediction_recommendation(context).await
            }
        }
    }

    fn provider(&self) -> LlmProvider {
        self.primary.provider()
    }
}

// ---------------------------------------------------------------------------
// HeuristicFallback: deterministic, no LLM
// ---------------------------------------------------------------------------

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
            confidence: Decimal::new(5, 1),
            rationale: "Heuristic fallback: no LLM available, holding position"
                .into(),
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
        LlmProvider::Grok
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trade_response_valid_json() {
        let raw = r#"{"action": "Buy", "size_percent": 25.0, "confidence": 0.85, "rationale": "Strong momentum"}"#;
        let rec = parse_trade_response(raw, "BTC/USDC").unwrap();
        assert_eq!(rec.action, TradeAction::Buy);
        assert!(rec.confidence > Decimal::new(8, 1));
        assert_eq!(rec.pair, "BTC/USDC");
    }

    #[test]
    fn parse_trade_response_with_fences() {
        let raw = "```json\n{\"action\": \"Sell\", \"size_percent\": 10, \"confidence\": 0.7, \"rationale\": \"Bearish\"}\n```";
        let rec = parse_trade_response(raw, "ETH/USDC").unwrap();
        assert_eq!(rec.action, TradeAction::Sell);
    }

    #[test]
    fn parse_trade_response_defaults_to_hold() {
        let raw = r#"{"action": "unknown", "confidence": 0.3}"#;
        let rec = parse_trade_response(raw, "BTC/USDC").unwrap();
        assert_eq!(rec.action, TradeAction::Hold);
    }

    #[test]
    fn parse_prediction_response_buy_yes() {
        let raw = r#"{"action": "BuyYes", "size_usdc": 50.0, "confidence": 0.8, "rationale": "High probability event"}"#;
        let rec = parse_prediction_response(raw, "mkt-123").unwrap();
        assert_eq!(rec.action, PredictionAction::BuyYes);
        assert_eq!(rec.market_id, "mkt-123");
    }

    #[test]
    fn parse_prediction_response_invalid_json() {
        let raw = "not json at all";
        assert!(parse_prediction_response(raw, "mkt-1").is_err());
    }

    #[test]
    fn strip_json_fences_works() {
        assert_eq!(strip_json_fences("```json\n{}\n```"), "{}");
        assert_eq!(strip_json_fences("```\n{}\n```"), "{}");
        assert_eq!(strip_json_fences("{}"), "{}");
        assert_eq!(
            strip_json_fences("  ```json\n{\"a\":1}\n```  "),
            "{\"a\":1}"
        );
    }
}
