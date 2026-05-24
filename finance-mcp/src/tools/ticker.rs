use std::sync::Arc;

use anyhow::Result;
use finance::portfolio::{model::Ticker, RestClient};

/// Fetch 24-hour price statistics for `symbol` (e.g. "BTC/USD").
pub async fn get_ticker(
    api: &Arc<RestClient>,
    symbol: &str,
) -> Result<Ticker> {
    api.ticker(symbol).await
}
