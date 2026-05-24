use std::sync::Arc;

use anyhow::Result;
use finance::portfolio::{
    model::{AccountInformation, Deposit, Order, Trade},
    RestClient,
};

/// Fetch account information including all asset balances.
pub async fn get_account(api: &Arc<RestClient>) -> Result<AccountInformation> {
    let ts = api.time().await?;
    api.account(ts).await
}

/// Fetch deposit history for the account.
pub async fn get_deposits(api: &Arc<RestClient>) -> Result<Vec<Deposit>> {
    let ts = api.time().await?;
    api.deposits(ts).await
}

/// Fetch trade history for a given symbol (e.g. "BTC/USD").
pub async fn get_my_trades(
    api: &Arc<RestClient>,
    symbol: &str,
) -> Result<Vec<Trade>> {
    let ts = api.time().await?;
    api.my_trades(symbol, ts).await
}

/// Fetch a specific order by its ID.
pub async fn fetch_order(
    api: &Arc<RestClient>,
    order_id: &str,
) -> Result<Order> {
    let ts = api.time().await?;
    api.fetch_order(order_id, ts).await
}

/// Fetch open orders. Pass `None` for `symbol` to fetch all open orders.
pub async fn get_open_orders(
    api: &Arc<RestClient>,
    symbol: Option<&str>,
) -> Result<Vec<Order>> {
    let ts = api.time().await?;
    api.open_orders(symbol, ts).await
}
