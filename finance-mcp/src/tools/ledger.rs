use std::sync::Arc;

use anyhow::Result;
use finance::portfolio::{model::{LedgerEntry, Transaction}, RestClient};

/// Fetch the complete ledger for a currency (auto-paginated).
/// Pass `None` for `currency` to fetch all currencies.
pub async fn get_full_ledger(
    api: &Arc<RestClient>,
    currency: Option<&str>,
) -> Result<Vec<LedgerEntry>> {
    let ts = api.time().await?;
    api.fetch_full_ledger(currency, ts).await
}

/// Fetch all deposit/withdrawal transactions (auto-paginated).
pub async fn get_all_transactions(
    api: &Arc<RestClient>,
) -> Result<Vec<Transaction>> {
    let ts = api.time().await?;
    api.fetch_all_transactions(ts).await
}
