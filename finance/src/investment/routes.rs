use std::collections::HashMap;

use crate::investment::RestClient;

#[derive(Clone, serde::Serialize, schemars::JsonSchema)]
pub struct Portfolio {
    pub can_trade: bool,
    pub can_withdraw: bool,
    pub can_deposit: bool,

    pub current_volume: f64,
    pub historical_volume: f64,
    pub total_fee_spending: f64,

    pub total_withdrawal: f64,
}

//fixme: only long operations are supported
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug)]
pub struct AssetTrade {
    pub entry_price: f64,
    pub amount: f64,
}

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug)]
pub struct Asset {
    pub name: Option<String>,
    pub symbol: String,

    //summary about position
    pub amount: f64,
    pub cost: f64,
    pub profit_loss: f64,

    pub average_entry_price: f64,

    pub trades: Vec<AssetTrade>,
    //todo: metrics?
}

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug)]
pub struct OwningAssets {
    pub assets: Vec<Asset>,
}

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

        current_volume += estimate_price_in_usd(api, &b.asset, b.free).await?;
    }

    //not included fee for banks to deposit account
    let mut total_fee = 0.0;
    let mut total_withdrawal = 0.0;
    let mut historical_volume = 0.0;
    let server_ts = api.time().await?;
    let entries = api.fetch_full_ledger(None, server_ts).await?;

    for e in entries {
        let amount = if e.currency == "USD" {
            e.amount.abs()
        } else {
            let new_amount =
                estimate_price_in_usd(api, &e.currency, e.amount.abs()).await?;

            tracing::info!(
                "new_amount = {new_amount}, old_amount = {}",
                e.amount.abs()
            );

            new_amount
        };

        match e.ty {
            super::LedgerEntryType::Swap | super::LedgerEntryType::Trade => {
                //skip swap between tokens
            }
            super::LedgerEntryType::Deposit => {
                historical_volume += amount;
            }
            super::LedgerEntryType::Withdrawal => {
                total_withdrawal += amount;
            }
            super::LedgerEntryType::TradeCommission
            | super::LedgerEntryType::ExchangeCommission => {
                total_fee += amount;
            }
        }
    }

    //exchange commission
    Ok(Portfolio {
        current_volume,
        historical_volume,
        total_withdrawal,
        total_fee_spending: total_fee,

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
    let (base_to_quote, trade_symbol) = if symbol == "BYN" {
        (false, "USD/BYN".to_string())
    } else {
        let ts = api.time().await?;

        let exchanges = api.exchange_info(ts).await?;

        let trade_info = exchanges
            .symbols
            .into_iter()
            .filter(|s| s.symbol.contains(symbol))
            .min_by_key(|s| s.symbol.len())
            .ok_or_else(|| {
                tracing::warn!("Failed to find trade of {symbol}");

                anyhow::anyhow!("no trading pair found for symbol {}", symbol)
            })?;

        (trade_info.quote_asset == "USD", trade_info.symbol)
    };

    tracing::info!("found trading pair {} for symbol {}", trade_symbol, symbol);

    let ticker = api.ticker(&trade_symbol).await?;

    if base_to_quote {
        Ok(amount * ticker.bid_price)
    } else {
        Ok(amount / ticker.bid_price)
    }
}

pub async fn find_quote_symbol(
    api: &RestClient,
    _base_symbol: &str,
) -> anyhow::Result<Option<String>> {
    let _ts = api.time().await?;

    //fetch pair from exchange-info
    todo!()
}

//  Leverage + Assets
pub async fn fetch_owning_assets(
    api_client: &RestClient,
) -> anyhow::Result<OwningAssets> {
    let server_ts = api_client.time().await?;

    let (_account, positions, currencies) = tokio::try_join!(
        api_client.account(server_ts),
        api_client.trading_positions(server_ts),
        api_client.currencies(server_ts),
    )?;

    // displaySymbol → full name. Leverage-only instruments may be absent.
    let name_by_symbol: HashMap<String, String> =
        currencies.into_iter().map(|c| (c.symbol, c.name)).collect();

    // assets.extend(account.balances.into_iter().map(|b| AssetExt::Owned(b)));

    let mut assets_by_symbol = HashMap::<String, Asset>::new();

    for position in positions {
        assert_eq!(
            position.close_price, 0.0,
            "Only long operations are supported"
        );

        let symbol = position
            .symbol
            .strip_suffix(".")
            .unwrap_or(&position.symbol)
            .to_string();

        // Some position symbols exist only in leverage mode and are not listed
        // under /currencies — leave name unset in that case.
        let name = {
            match name_by_symbol.get(&symbol).cloned() {
                Some(name) => Some(name),
                None => symbol
                    .contains("/USD_LEVERAGE")
                    .then(|| symbol.strip_suffix("/USD_LEVERAGE"))
                    .flatten()
                    .and_then(|s| s.to_string().into()),
            }
        };

        assets_by_symbol
            .entry(symbol.clone())
            .and_modify(|a| {
                a.profit_loss += position.profit_loss;
                a.cost += position.cost;
                a.amount += position.open_qty;
                a.average_entry_price += position.open_price;

                a.trades.push(AssetTrade {
                    entry_price: position.open_price,
                    amount: position.open_qty,
                });
            })
            .or_insert(Asset {
                symbol,
                name,

                amount: position.open_qty,
                cost: position.cost,
                profit_loss: position.profit_loss,
                average_entry_price: position.open_price,

                trades: vec![AssetTrade {
                    amount: position.open_qty,
                    entry_price: position.open_price,
                }],
            });
    }

    let leverage_assets = assets_by_symbol
        .into_values()
        .map(|mut a| {
            assert!(!a.trades.is_empty());

            a.average_entry_price /= a.trades.len() as f64;

            a
        })
        .collect::<Vec<_>>();

    Ok(OwningAssets {
        assets: leverage_assets,
    })
}

pub fn new_pair(base: &str, quote: &str) -> String {
    format!("{}/{}", base, quote)
}
