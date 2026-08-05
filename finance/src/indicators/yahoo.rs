use std::time::SystemTime;

use yahoo_finance_api as yahoo;

use super::{AnalysisConfig, TechnicalIndicators, compute_snapshot};

/// Fetch daily history from Yahoo and compute a technical snapshot.
pub async fn snapshot_from_yahoo(
    symbol: &str,
    config: &AnalysisConfig,
) -> Option<TechnicalIndicators> {
    let provider = yahoo::YahooConnector::new().ok()?;
    let start =
        SystemTime::now() - std::time::Duration::from_secs(180 * 24 * 60 * 60);
    let response = provider
        .get_quote_history(symbol, start.into(), SystemTime::now().into())
        .await
        .ok()?;
    let quotes = response.quotes().ok()?;
    if quotes.is_empty() {
        return None;
    }
    let closes: Vec<f64> = quotes.iter().map(|q| q.close).collect();
    compute_snapshot(&closes, config)
}
