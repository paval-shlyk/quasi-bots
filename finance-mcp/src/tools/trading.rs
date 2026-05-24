use std::sync::Arc;

use anyhow::Result;
use finance::portfolio::{
    model::{TradingPosition, TradingPositionHistory},
    RestClient,
};

/// Fetch currently open trading (CFD/margin) positions.
pub async fn get_trading_positions(
    api: &Arc<RestClient>,
) -> Result<Vec<TradingPosition>> {
    let ts = api.time().await?;
    api.trading_positions(ts).await
}

/// Fetch closed trading position history.
/// Pass `None` for `symbol` to fetch history for all symbols.
pub async fn get_trading_position_history(
    api: &Arc<RestClient>,
    symbol: Option<&str>,
) -> Result<Vec<TradingPositionHistory>> {
    let ts = api.time().await?;
    api.trading_position_history(symbol, ts).await
}
