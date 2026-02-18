use serde::{Deserialize, Serialize};

/// Authentication request payload for WebSocket.
#[derive(Serialize, Deserialize, Debug)]
pub struct AuthRequest {
    pub op: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(rename = "signature")]
    pub signature: String,
    pub timestamp: u64,
}

/// Authentication response from WebSocket.
#[derive(Serialize, Deserialize, Debug)]
pub struct AuthResponse {
    pub success: bool,
    pub reason: Option<String>,
}

/// Represents a trading position.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub avg_price: f64,
}

/// Snapshot of the entire portfolio.
#[derive(Serialize, Deserialize, Debug)]
pub struct PortfolioSnapshot {
    pub positions: Vec<Position>,
    pub updated_at: u64,
}

/// Enum representing possible events received from the WebSocket stream.
#[derive(Serialize, Deserialize, Debug)]
pub enum PortfolioEvent {
    Auth(AuthResponse),
    Snapshot(PortfolioSnapshot),
    PositionUpdate(Position),
    Raw(serde_json::Value),
}

/// Server time response.
#[derive(Serialize, Deserialize, Debug)]
pub struct ServerTime {
    #[serde(alias = "serverTime", alias = "server_time")]
    pub server_time: u64,
}

/// Currency information.
#[derive(Serialize, Deserialize, Debug)]
pub struct Currency {
    pub id: String,
    pub name: String,
    // Add other fields as discovered/needed
}

/// Order book entry (price, quantity).
/// Represented as strings in many APIs to preserve precision, but here we use f64 for simplicity
/// or keep as String if the API returns strings. Dzengi/Binance usually return strings for precision.
/// Let's stick to strict types where possible, but use String for numbers if unsure about precision.
/// Actually, for financial apps, `rust_decimal` is better, but `f64` is requested in previous DTOs.
/// Let's use `f64` for now to match `Position`.
#[derive(Serialize, Debug)]
pub struct OrderBookEntry {
    pub price: f64,
    pub quantity: f64,
}

// Custom deserialization for OrderBookEntry from array if needed.
// Many APIs return [price, quantity] as an array.
// We need to support that.
use serde::de::{self, SeqAccess, Visitor};
use std::fmt;

impl<'de> Deserialize<'de> for OrderBookEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OrderBookEntryVisitor;

        impl<'de> Visitor<'de> for OrderBookEntryVisitor {
            type Value = OrderBookEntry;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("an order book entry array [price, quantity]")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                // Try to deserialize elements as StringOrNumber helper first to handle both string/number types
                // But StringOrNumber is private. We can replicate logic or use serde_json::Value.
                // Using serde_json::Value is safer for mixed types.

                let price_val: serde_json::Value = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let qty_val: serde_json::Value = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;

                let price = if let Some(n) = price_val.as_f64() {
                    n
                } else if let Some(s) = price_val.as_str() {
                    s.parse().map_err(de::Error::custom)?
                } else {
                    return Err(de::Error::custom(
                        "price is not a number or string",
                    ));
                };

                let quantity = if let Some(n) = qty_val.as_f64() {
                    n
                } else if let Some(s) = qty_val.as_str() {
                    s.parse().map_err(de::Error::custom)?
                } else {
                    return Err(de::Error::custom(
                        "quantity is not a number or string",
                    ));
                };

                Ok(OrderBookEntry { price, quantity })
            }
        }

        deserializer.deserialize_seq(OrderBookEntryVisitor)
    }
}

/// Order book depth.
#[derive(Serialize, Deserialize, Debug)]
pub struct OrderBook {
    pub bids: Vec<OrderBookEntry>,
    pub asks: Vec<OrderBookEntry>,
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: Option<u64>,
}

/// Symbol information from exchange info.
#[derive(Serialize, Deserialize, Debug)]
pub struct SymbolInfo {
    pub symbol: String,
    pub status: String,
    #[serde(rename = "baseAsset")]
    pub base_asset: String,
    #[serde(rename = "quoteAsset")]
    pub quote_asset: String,
    // Add filters and other fields as needed
}

/// Exchange information.
#[derive(Serialize, Deserialize, Debug)]
pub struct ExchangeInfo {
    #[serde(rename = "timezone")]
    pub timezone: Option<String>,
    #[serde(rename = "serverTime")]
    pub server_time: Option<u64>,
    pub symbols: Vec<SymbolInfo>,
}

/// Kline (candlestick) data.
/// Often returned as array of arrays. We might need a custom deserializer or a struct wrapper.
/// If it's an array of arrays: `[open_time, open, high, low, close, volume, close_time, ...]`
#[derive(Serialize, Debug)]
pub struct Kline {
    pub open_time: u64,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub open: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub high: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub low: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub close: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub volume: String,
    pub close_time: Option<u64>,
    // ... other fields
}

// Custom deserialization for Kline from array might be needed if the API returns arrays.
// For now, let's assume the user handles the `Value` -> `Kline` conversion or the API returns objects.
// Note: most crypto APIs (Binance, etc) return arrays for klines.
// We'll define a helper struct or keep using serde_json::Value for klines if it's too complex for now,
// but the prompt asked for "strict type system".
// Let's implement a custom deserializer for Kline.

impl<'de> Deserialize<'de> for Kline {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct KlineVisitor;

        impl<'de> Visitor<'de> for KlineVisitor {
            type Value = Kline;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a kline array")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let open_time = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                // Helper to parse next element as string or number -> string
                let next_as_string = |seq: &mut A| -> Result<String, A::Error> {
                    let v: serde_json::Value = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::custom("missing element"))?;
                    match v {
                        serde_json::Value::String(s) => Ok(s),
                        serde_json::Value::Number(n) => Ok(n.to_string()),
                        _ => {
                            Err(de::Error::custom("expected string or number"))
                        }
                    }
                };

                let open = next_as_string(&mut seq)?;
                let high = next_as_string(&mut seq)?;
                let low = next_as_string(&mut seq)?;
                let close = next_as_string(&mut seq)?;
                let volume = next_as_string(&mut seq)?;

                let close_time = seq.next_element::<u64>()?;

                // consume remaining elements if any
                while seq.next_element::<serde_json::Value>()?.is_some() {}

                Ok(Kline {
                    open_time,
                    open,
                    high,
                    low,
                    close,
                    volume,
                    close_time,
                })
            }
        }

        deserializer.deserialize_seq(KlineVisitor)
    }
}

/// Account balance.
#[derive(Serialize, Deserialize, Debug)]
pub struct Balance {
    pub asset: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub free: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub locked: String,
    pub timestamp: Option<u64>,
}

fn deserialize_string_from_number<'de, D>(
    deserializer: D,
) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Number(f64),
    }

    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::String(s) => Ok(s),
        StringOrNumber::Number(n) => Ok(n.to_string()),
    }
}

fn deserialize_option_string_from_number<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumberOption {
        String(String),
        Number(f64),
        None,
    }

    match Option::<StringOrNumberOption>::deserialize(deserializer)? {
        Some(StringOrNumberOption::String(s)) => Ok(Some(s)),
        Some(StringOrNumberOption::Number(n)) => Ok(Some(n.to_string())),
        Some(StringOrNumberOption::None) | None => Ok(None),
    }
}

/// Account information.
#[derive(Serialize, Deserialize, Debug)]
pub struct AccountInformation {
    #[serde(rename = "makerCommission")]
    pub maker_commission: Option<f64>,
    #[serde(rename = "takerCommission")]
    pub taker_commission: Option<f64>,
    #[serde(rename = "canTrade")]
    pub can_trade: Option<bool>,
    #[serde(rename = "canWithdraw")]
    pub can_withdraw: Option<bool>,
    #[serde(rename = "canDeposit")]
    pub can_deposit: Option<bool>,
    #[serde(rename = "updateTime")]
    pub update_time: Option<u64>,
    pub balances: Vec<Balance>,
}

/// Deposit record.
#[derive(Serialize, Deserialize, Debug)]
pub struct Deposit {
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub amount: String,
    #[serde(alias = "currency", alias = "asset")]
    pub coin: String,
    pub network: Option<String>,
    #[serde(deserialize_with = "deserialize_status")]
    pub status: i32,
    pub address: Option<String>,
    pub tx_id: Option<String>,
    #[serde(
        alias = "insertTime",
        alias = "createTime",
        alias = "timestamp",
        alias = "time"
    )]
    pub timestamp: u64,
}

fn deserialize_status<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Status {
        Integer(i32),
        String(String),
    }

    match Status::deserialize(deserializer)? {
        Status::Integer(i) => Ok(i),
        Status::String(s) => match s.as_str() {
            "PROCESSED" => Ok(1), // Mapping PROCESSED to 1 (success)
            "PENDING" => Ok(0),
            "REJECTED" => Ok(2),
            _ => Ok(-1), // Unknown status
        },
    }
}

/// Trade record.
#[derive(Serialize, Deserialize, Debug)]
pub struct Trade {
    pub symbol: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub id: String,
    #[serde(
        rename = "orderId",
        deserialize_with = "deserialize_string_from_number"
    )]
    pub order_id: String,
    pub price: String,
    pub qty: String,
    #[serde(rename = "quoteQty")]
    pub quote_qty: Option<String>,
    pub commission: Option<String>,
    #[serde(rename = "commissionAsset")]
    pub commission_asset: Option<String>,
    pub time: u64,
    #[serde(rename = "isBuyer")]
    pub is_buyer: bool,
    #[serde(rename = "isMaker")]
    pub is_maker: bool,
    #[serde(rename = "isBestMatch")]
    pub is_best_match: Option<bool>,
}

/// Order information.
#[derive(Serialize, Deserialize, Debug)]
pub struct Order {
    pub symbol: String,
    #[serde(
        rename = "orderId",
        deserialize_with = "deserialize_string_from_number"
    )]
    pub order_id: String,
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    pub price: String,
    #[serde(rename = "origQty")]
    pub orig_qty: String,
    #[serde(rename = "executedQty")]
    pub executed_qty: String,
    #[serde(rename = "cummulativeQuoteQty")]
    pub cummulative_quote_qty: String,
    pub status: String,
    #[serde(rename = "timeInForce")]
    pub time_in_force: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub side: String,
    #[serde(rename = "stopPrice")]
    pub stop_price: Option<String>,
    pub time: u64,
    #[serde(rename = "updateTime")]
    pub update_time: u64,
}

/// Ledger entry.
#[derive(Serialize, Deserialize, Debug)]
pub struct LedgerEntry {
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub id: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub amount: String,
    pub currency: String,
    pub description: Option<String>,
    pub status: String,
    pub timestamp: u64,
    #[serde(rename = "type")]
    pub type_: String,
}

/// Transaction (often similar to Ledger but might have specific fields for deposits/withdrawals).
#[derive(Serialize, Deserialize, Debug)]
pub struct Transaction {
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub id: String,
    #[serde(deserialize_with = "deserialize_string_from_number")]
    pub amount: String,
    pub currency: String,
    pub status: String,
    pub timestamp: u64,
    #[serde(rename = "type")]
    pub type_: String,
    pub address: Option<String>,
    #[serde(rename = "txId")]
    pub tx_id: Option<String>,
}

/// Trading position details (rest endpoint might return different structure than WS Position).
#[derive(Serialize, Deserialize, Debug)]
pub struct TradingPosition {
    pub symbol: String,
    #[serde(
        default,
        deserialize_with = "deserialize_option_string_from_number"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_string_from_number",
        alias = "quantity",
        alias = "size"
    )]
    pub amount: Option<String>,
    #[serde(
        default,
        rename = "openPrice",
        deserialize_with = "deserialize_option_string_from_number"
    )]
    pub open_price: Option<String>,
    #[serde(
        default,
        rename = "currentPrice",
        deserialize_with = "deserialize_option_string_from_number"
    )]
    pub current_price: Option<String>,
    #[serde(
        default,
        rename = "pl",
        deserialize_with = "deserialize_option_string_from_number"
    )]
    pub profit_loss: Option<String>,
    pub timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TradingPositionsResponse {
    pub positions: Vec<TradingPosition>,
}

/// Trading position history.
#[derive(Serialize, Deserialize, Debug)]
pub struct TradingPositionHistory {
    pub symbol: String,
    #[serde(
        default,
        deserialize_with = "deserialize_option_string_from_number"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_string_from_number",
        alias = "quantity",
        alias = "size"
    )]
    pub amount: Option<String>,
    #[serde(
        default,
        rename = "openPrice",
        deserialize_with = "deserialize_option_string_from_number"
    )]
    pub open_price: Option<String>,
    #[serde(
        default,
        rename = "closePrice",
        deserialize_with = "deserialize_option_string_from_number"
    )]
    pub close_price: Option<String>,
    #[serde(
        default,
        rename = "pl",
        deserialize_with = "deserialize_option_string_from_number"
    )]
    pub profit_loss: Option<String>,
    #[serde(rename = "openTimestamp")]
    pub open_timestamp: Option<u64>,
    #[serde(rename = "closeTimestamp")]
    pub close_timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TradingPositionHistoryResponse {
    pub history: Vec<TradingPositionHistory>,
}
