use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Market data types
// ---------------------------------------------------------------------------

/// Metadata for a single Polymarket event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub market_id: String,
    pub condition_id: String,
    pub title: String,
    pub description: String,
    pub end_date: Option<DateTime<Utc>>,
    pub category: Option<String>,
    pub active: bool,
}

/// Current CLOB state for a market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOrderBook {
    pub market_id: String,
    pub yes_price: Decimal,
    pub no_price: Decimal,
    pub yes_bid: Decimal,
    pub yes_ask: Decimal,
    pub no_bid: Decimal,
    pub no_ask: Decimal,
    pub volume_24h: Decimal,
    pub liquidity: Decimal,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredictionSide {
    Yes,
    No,
}

impl std::fmt::Display for PredictionSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Yes => f.write_str("yes"),
            Self::No => f.write_str("no"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredictionOrderAction {
    Buy,
    Sell,
}

impl std::fmt::Display for PredictionOrderAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Buy => f.write_str("buy"),
            Self::Sell => f.write_str("sell"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredictionStatus {
    Placed,
    Filled,
    Cancelled,
    Failed,
}

impl std::fmt::Display for PredictionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Placed => f.write_str("placed"),
            Self::Filled => f.write_str("filled"),
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed => f.write_str("failed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Signal & result types
// ---------------------------------------------------------------------------

/// Signal emitted by [`PolymarketStrategy`] after LLM + heuristic analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionSignal {
    pub market_id: String,
    pub market_title: String,
    pub side: PredictionSide,
    pub action: PredictionOrderAction,
    pub shares: Decimal,
    pub limit_price: Option<Decimal>,
    pub confidence: Decimal,
    pub rationale: String,
    pub timestamp: DateTime<Utc>,
}

/// Execution report returned by [`PolymarketExecutor`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    pub order_id: String,
    pub market_id: String,
    pub side: PredictionSide,
    pub action: PredictionOrderAction,
    pub filled_shares: Decimal,
    pub avg_price: Decimal,
    pub total_cost: Decimal,
    pub status: PredictionStatus,
    pub timestamp: DateTime<Utc>,
}

/// Edge between LLM-assessed probability and market-implied probability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEdge {
    pub market_id: String,
    pub llm_probability: Decimal,
    pub market_implied_probability: Decimal,
    /// Positive = LLM thinks the market underprices this outcome.
    pub edge: Decimal,
    pub side: PredictionSide,
}
