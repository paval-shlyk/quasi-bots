use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    pub pair: String,
    pub price: Decimal,
    pub bid: Decimal,
    pub ask: Decimal,
    pub volume_24h: Decimal,
    pub high_24h: Decimal,
    pub low_24h: Decimal,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalIndicators {
    pub rsi: Option<Decimal>,
    pub macd: Option<MACDValues>,
    pub bollinger: Option<BollingerBands>,
    pub sma_20: Option<Decimal>,
    pub ema_12: Option<Decimal>,
    pub ema_26: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MACDValues {
    pub macd_line: Decimal,
    pub signal_line: Decimal,
    pub histogram: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BollingerBands {
    pub upper: Decimal,
    pub middle: Decimal,
    pub lower: Decimal,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    Buy,
    Sell,
}

impl std::fmt::Display for TradeSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Buy => f.write_str("buy"),
            Self::Sell => f.write_str("sell"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Market => f.write_str("market"),
            Self::Limit => f.write_str("limit"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeStatus {
    Pending,
    Filled,
    PartiallyFilled,
    Cancelled,
    Failed,
}

impl std::fmt::Display for TradeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::Filled => f.write_str("filled"),
            Self::PartiallyFilled => f.write_str("partial"),
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed => f.write_str("failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    Long,
    Short,
}

impl std::fmt::Display for PositionSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Long => f.write_str("long"),
            Self::Short => f.write_str("short"),
        }
    }
}

// Trade signal & result

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    pub pair: String,
    pub side: TradeSide,
    pub order_type: OrderType,
    pub quantity: Decimal,
    /// `None` for market orders.
    pub price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub confidence: Decimal,
    pub rationale: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeResult {
    pub order_id: String,
    pub pair: String,
    pub side: TradeSide,
    pub filled_quantity: Decimal,
    pub avg_fill_price: Decimal,
    pub fee: Decimal,
    pub status: TradeStatus,
    pub timestamp: DateTime<Utc>,
}
