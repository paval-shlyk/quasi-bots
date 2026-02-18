use axum::{Json, extract::State, response::IntoResponse};

use crate::portfolio::{Balance, RestClient, TradingPosition};

#[derive(Clone, serde::Serialize)]
pub struct Portfolio {
    pub can_trade: bool,
    pub can_withdraw: bool,
    pub can_deposit: bool,

    pub current_volume: f64,
    pub historical_volume: f64,
    pub total_fee_spending: f64,
}

pub enum Asset {
    Owned(Balance),
    Leverage(TradingPosition),
}

pub async fn get_portfolio(
    State(state): State<crate::FinanceState>,
) -> impl IntoResponse {
    fetch_portfolio(&state.api).await.map(Json).map_err(|e| {
        tracing::error!("failed to fetch portfolio: {:?}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_historical_volume() -> impl IntoResponse {}

pub async fn fetch_portfolio(api: &RestClient) -> anyhow::Result<Portfolio> {
    let server_ts = api.time().await?;

    let account = api.account(server_ts).await?;

    let mut current_volume = 0.0;

    for b in account.balances {
        tracing::info!("Fetching information for {}", b.asset);

        if b.asset == "USD" {
            current_volume += b.free;
            current_volume += b.locked; // locked balance is also part of the portfolio, as it
            // represents an asset that we own, but is currently not
            // available for trading (e.g. because it's used as
            // collateral for a margin position, or because it's part
            // of an open order). So we should include it in the
            // current volume calculation.
            continue;
        }

        current_volume += estimate_price_in_usd(&api, &b.asset, b.free).await?;
    }

    // let ledger_entries = api.ledger(None, server_ts).await?;

    tracing::info!("BIBA");

    //exchange commission
    Ok(Portfolio {
        current_volume,
        historical_volume: 0.0,
        total_fee_spending: 0.0,

        can_trade: account.can_trade,
        can_withdraw: account.can_withdraw,
        can_deposit: account.can_deposit,
    })
}

pub async fn estimate_price_in_usd(
    api: &RestClient,
    symbol: &str,
    amount: f64,
) -> anyhow::Result<f64> {
    if symbol == "USD" {
        return Ok(amount);
    }

    let ts = api.time().await?;

    let exchanges = api.exchange_info(ts).await?;

    let trade_symbol = exchanges
        .symbols
        .into_iter()
        .filter(|s| s.symbol.contains(symbol))
        .min_by_key(|s| s.symbol.len())
        .map(|s| s.symbol)
        .ok_or_else(|| {
            tracing::warn!("Failed to find trade of {symbol}");

            anyhow::anyhow!("no trading pair found for symbol {}", symbol)
        })?;

    tracing::info!("found trading pair {} for symbol {}", trade_symbol, symbol);

    let ticker = api.ticker(&trade_symbol).await?;

    Ok(ticker.bid_price * amount)
}

pub async fn find_quote_symbol(
    api: &RestClient,
    base_symbol: &str,
) -> anyhow::Result<Option<String>> {
    let ts = api.time().await?;

    //fetch pair from exchange-info
    todo!()
}

pub fn new_pair(base: &str, quote: &str) -> String {
    format!("{}/{}", base, quote)
}
