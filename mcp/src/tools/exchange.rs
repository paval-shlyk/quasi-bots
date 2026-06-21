use std::sync::Arc;

use anyhow::Result;
use finance::portfolio::{
    model::{Currency, ExchangeInfo, Kline, OrderBook},
    RestClient,
};

/// Fetch current server time (Unix ms).
pub async fn get_server_time(api: &Arc<RestClient>) -> Result<u64> {
    api.time().await
}

/// Fetch the full list of supported currencies.
pub async fn get_currencies(api: &Arc<RestClient>) -> Result<Vec<Currency>> {
    let ts = api.time().await?;
    api.currencies(ts).await
}

/// Fetch the current order book (bids/asks) for `symbol`.
pub async fn get_order_book(
    api: &Arc<RestClient>,
    symbol: &str,
) -> Result<OrderBook> {
    api.depth(symbol).await
}

/// Fetch exchange metadata: all trading pairs, their status and assets.
pub async fn get_exchange_info(api: &Arc<RestClient>) -> Result<ExchangeInfo> {
    let ts = api.time().await?;
    api.exchange_info(ts).await
}

/// Fetch candlestick (OHLCV) data for `symbol` at the given `interval`
/// (e.g. "1m", "5m", "1h", "1d").
pub async fn get_klines(
    api: &Arc<RestClient>,
    symbol: &str,
    interval: &str,
) -> Result<Vec<Kline>> {
    api.klines(symbol, interval).await
}
