use std::collections::HashMap;

use crate::{
    TradingPosition,
    investment::{Balance, ExchangeInfo, RestClient, Trade},
};

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
#[derive(
    Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug,
)]
pub struct AssetEntryTrade {
    pub entry_price: f64,
    pub amount: f64,
}

#[derive(
    Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug,
)]
pub struct Asset {
    pub name: Option<String>,
    pub symbol: String,

    // summary about position
    pub amount: f64,
    /// Live market value of the holding (`unit_market_price * amount`).
    pub cost: f64,
    pub profit_loss: f64,
    pub profit_lost_pct: f64,

    pub average_entry_price: f64,
    pub unit_market_price: f64,
    pub distance_from_entry_price_pct: f64,

    pub trades: Vec<AssetEntryTrade>,
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

const QTY_EPS: f64 = 1e-12;

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
    let (trade_symbol, asset_is_base) = resolve_trade_pair(&exchanges, symbol)
        .ok_or_else(|| {
            tracing::warn!("Failed to find trade of {symbol}");
            anyhow::anyhow!("no trading pair found for symbol {}", symbol)
        })?;

    tracing::info!("found trading pair {} for symbol {}", trade_symbol, symbol);

    let ticker = api.ticker(&trade_symbol).await?;

    if asset_is_base {
        Ok(amount * ticker.bid_price)
    } else {
        Ok(amount / ticker.bid_price)
    }
}

/// Resolve a trade pair for a held asset; second value is whether that asset is base.
///
/// Tickers are quote-per-base, so when the asset is quote the caller must invert:
/// mark as `amount / bid`, entry as `1/price`, and treat sells as acquisitions.
/// Prefer base/USD when several pairs exist so valuation stays in USD without a chain.
pub fn resolve_trade_pair(
    info: &ExchangeInfo,
    asset: &str,
) -> Option<(String, bool)> {
    if asset == "BYN" {
        return Some(("USD/BYN".to_string(), false));
    }

    if let Some(s) = info
        .symbols
        .iter()
        .filter(|s| s.base_asset == asset && s.quote_asset == "USD")
        .min_by_key(|s| s.symbol.len())
    {
        return Some((s.symbol.clone(), true));
    }

    if let Some(s) = info
        .symbols
        .iter()
        .filter(|s| s.base_asset == asset)
        .min_by_key(|s| s.symbol.len())
    {
        return Some((s.symbol.clone(), true));
    }

    if let Some(s) = info
        .symbols
        .iter()
        .filter(|s| s.quote_asset == asset)
        .min_by_key(|s| s.symbol.len())
    {
        return Some((s.symbol.clone(), false));
    }

    info.symbols
        .iter()
        .filter(|s| s.symbol.contains(asset))
        .min_by_key(|s| s.symbol.len())
        .map(|s| (s.symbol.clone(), s.base_asset == asset))
}

pub async fn find_quote_symbol(
    api: &RestClient,
    base_symbol: &str,
) -> anyhow::Result<Option<String>> {
    let ts = api.time().await?;
    let info = api.exchange_info(ts).await?;
    Ok(resolve_trade_pair(&info, base_symbol).map(|(pair, _)| pair))
}

/// Open lots from `/myTrades`, newest first, until `balance_qty` is covered.
fn lots_from_trades(
    balance_qty: f64,
    mut trades: Vec<Trade>,
    asset_is_base: bool,
) -> Vec<AssetEntryTrade> {
    trades.sort_by_key(|t| std::cmp::Reverse(t.time));

    let mut remaining = balance_qty;
    let mut lots = Vec::new();

    for t in trades {
        if remaining <= QTY_EPS {
            break;
        }

        // Base: buy acquires qty. Quote: sell of base acquires quote amount.
        let is_acquisition = if asset_is_base {
            t.is_buyer
        } else {
            !t.is_buyer
        };
        if !is_acquisition {
            continue;
        }

        let price: f64 = t.price.parse().unwrap_or(0.0);
        let qty: f64 = if asset_is_base {
            t.qty.parse().unwrap_or(0.0)
        } else if let Some(ref qq) = t.quote_qty {
            qq.parse().unwrap_or(0.0)
        } else {
            t.qty.parse().unwrap_or(0.0) * price
        };

        // Quote holdings: invert pair price so cost is in base (USD for USD/*).
        let entry_price = if asset_is_base {
            price
        } else if price > 0.0 {
            1.0 / price
        } else {
            0.0
        };

        let take = qty.min(remaining);
        if take > QTY_EPS {
            lots.push(AssetEntryTrade {
                entry_price,
                amount: take,
            });
            remaining -= take;
        }
    }

    lots
}

fn normalize_symbol(symbol: &str) -> String {
    symbol.strip_suffix('.').unwrap_or(symbol).to_string()
}

/// Map exchange / leverage symbols to a lookup key (Yahoo, Finnhub, news).
/// e.g. `TSM.` → `TSM`, `TSM/USD_LEVERAGE` → `TSM`, `US500` → `^GSPC`.
pub fn lookup_symbol(symbol: &str) -> String {
    let s = normalize_symbol(symbol);
    let s = s.strip_suffix("/USD_LEVERAGE").unwrap_or(&s);

    // Broker CFD / index names → Yahoo-style symbols (technicals / news).
    // Analyst price targets still often missing for pure indices.
    match s {
        "US500" | "SPX" | "SP500" | "SPX500" => "^GSPC".into(),
        "US100" | "NDX" | "NAS100" | "USTEC" => "^NDX".into(),
        "US30" | "DJIA" | "DOW" | "WALLSTREET30" => "^DJI".into(),
        "DE40" | "DAX" | "GER40" => "^GDAXI".into(),
        "UK100" | "FTSE" | "UK100GBP" => "^FTSE".into(),
        "JP225" | "NI225" | "NIKKEI" => "^N225".into(),
        other => other.to_string(),
    }
}

fn resolve_asset_name(
    name_by_symbol: &HashMap<String, String>,
    symbol: &str,
) -> Option<String> {
    if let Some(name) = name_by_symbol.get(symbol) {
        return Some(name.clone());
    }
    // e.g. "TSM/USD_LEVERAGE" → look up "TSM"
    symbol
        .strip_suffix("/USD_LEVERAGE")
        .and_then(|base| name_by_symbol.get(base).cloned())
}

async fn spot_asset_from_balance(
    api: &RestClient,
    balance: &Balance,
    exchange_info: &ExchangeInfo,
    name_by_symbol: &HashMap<String, String>,
    server_ts: u64,
) -> anyhow::Result<Option<Asset>> {
    let amount = balance.free + balance.locked;
    if amount <= QTY_EPS || balance.asset == "USD" || balance.asset == "BYN" {
        return Ok(None);
    }

    let symbol = normalize_symbol(&balance.asset);
    let Some((trade_symbol, asset_is_base)) =
        resolve_trade_pair(exchange_info, &symbol)
    else {
        return Err(anyhow::anyhow!(
            "no trading pair for balance asset {symbol}",
        ));
    };

    let raw_trades =
        api.my_trades(&trade_symbol, server_ts).await.map_err(|e| {
            anyhow::anyhow!(
                "my_trades failed for {} ({}): {e}",
                symbol,
                trade_symbol
            )
        })?;

    let trades = lots_from_trades(amount, raw_trades, asset_is_base);
    let explained: f64 = trades.iter().map(|t| t.amount).sum();
    if explained + QTY_EPS < amount {
        return Err(anyhow::anyhow!(
            "myTrades only explain {explained} of {amount} for {symbol} \
             (pair {trade_symbol}); history may be truncated"
        ));
    }

    assert!(explained >= QTY_EPS);

    // Entry basis from reconstructed lots (not live mark).
    let entry_cost: f64 = trades.iter().map(|t| t.entry_price * t.amount).sum();
    let average_entry_price = entry_cost / explained;

    let ticker = api.ticker(&trade_symbol).await.map_err(|e| {
        anyhow::anyhow!("ticker failed for {trade_symbol}: {e}")
    })?;
    let unit_market_price = if asset_is_base {
        ticker.bid_price
    } else {
        assert!(ticker.bid_price > 0.0);

        1.0 / ticker.bid_price
    };

    let cost = amount * unit_market_price; // a live market value
    let profit_loss = cost - entry_cost;
    let profit_lost_pct = profit_loss / entry_cost * 100.0;
    let distance_from_entry_price_pct =
        (unit_market_price - average_entry_price) / average_entry_price * 100.0;

    Ok(Some(Asset {
        name: resolve_asset_name(name_by_symbol, &symbol),
        symbol,
        amount,
        cost,
        profit_loss,
        profit_lost_pct,
        average_entry_price,
        unit_market_price,
        distance_from_entry_price_pct,
        trades,
    }))
}

fn build_leverage_assets(
    positions: Vec<TradingPosition>,
    name_by_symbol: &HashMap<String, String>,
) -> Vec<Asset> {
    let mut assets_by_symbol = HashMap::<String, Asset>::new();

    for position in positions {
        assert_eq!(
            position.close_price, 0.0,
            "Only long operations are supported"
        );

        let symbol = normalize_symbol(&position.symbol);
        let name = resolve_asset_name(name_by_symbol, &symbol);

        if position.open_qty.abs() <= QTY_EPS {
            tracing::warn!(
                "Too small owning assets: {} count={}",
                position.symbol,
                position.open_qty
            );
            continue;
        }

        assets_by_symbol
            .entry(symbol.clone())
            .and_modify(|a| {
                a.profit_loss += position.profit_loss;
                a.cost += position.cost;
                a.amount += position.open_qty;

                a.trades.push(AssetEntryTrade {
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
                // Derived below after optional merge of same-symbol positions.
                profit_lost_pct: 0.0,
                average_entry_price: 0.0,
                unit_market_price: 0.0,
                distance_from_entry_price_pct: 0.0,
                trades: vec![AssetEntryTrade {
                    amount: position.open_qty,
                    entry_price: position.open_price,
                }],
            });
    }

    assets_by_symbol
        .into_values()
        .map(|mut a| {
            assert!(!a.trades.is_empty());

            // Entry notional from open lots; cost is live market value (exchange).
            let entry_cost: f64 =
                a.trades.iter().map(|t| t.entry_price * t.amount).sum();

            assert!(a.amount.abs() > QTY_EPS);

            a.average_entry_price = entry_cost / a.amount;
            a.unit_market_price = a.cost / a.amount;
            a.profit_lost_pct = a.profit_loss / entry_cost * 100.0;

            a.distance_from_entry_price_pct = (a.unit_market_price
                - a.average_entry_price)
                / a.average_entry_price
                * 100.0;
            a
        })
        .collect::<Vec<_>>()
}

/// Spot balances + leverage positions as raw holdings (no external analysis).
pub async fn fetch_holding_assets(
    api_client: &RestClient,
) -> anyhow::Result<Vec<Asset>> {
    let server_ts = api_client.time().await?;

    let (account, positions, currencies, exchange_info) = tokio::try_join!(
        api_client.account(server_ts),
        api_client.trading_positions(server_ts),
        api_client.currencies(server_ts),
        api_client.exchange_info(server_ts),
    )?;

    let name_by_symbol: HashMap<String, String> =
        currencies.into_iter().map(|c| (c.symbol, c.name)).collect();

    let mut spot_assets = Vec::new();

    for balance in &account.balances {
        let Some(asset) = spot_asset_from_balance(
            api_client,
            balance,
            &exchange_info,
            &name_by_symbol,
            server_ts,
        )
        .await?
        else {
            tracing::debug!(
                "Failed to resolve spot asset: {symbol}",
                symbol = balance.asset
            );
            continue;
        };

        spot_assets.push(asset);
    }

    let leverage_assets = build_leverage_assets(positions, &name_by_symbol);

    Ok([spot_assets, leverage_assets].concat())
}

pub fn new_pair(base: &str, quote: &str) -> String {
    format!("{}/{}", base, quote)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investment::{SymbolInfo, Trade};

    fn trade(time: u64, price: &str, qty: &str, is_buyer: bool) -> Trade {
        Trade {
            symbol: "BTC/USD".into(),
            id: time.to_string(),
            order_id: time.to_string(),
            price: price.into(),
            qty: qty.into(),
            quote_qty: None,
            commission: None,
            commission_asset: None,
            time,
            is_buyer,
            is_maker: false,
            is_best_match: None,
        }
    }

    fn trade_with_quote(
        time: u64,
        price: &str,
        qty: &str,
        quote_qty: &str,
        is_buyer: bool,
    ) -> Trade {
        let mut t = trade(time, price, qty, is_buyer);
        t.quote_qty = Some(quote_qty.into());
        t
    }

    fn symbol_info(symbol: &str, base: &str, quote: &str) -> SymbolInfo {
        SymbolInfo {
            symbol: symbol.into(),
            status: "TRADING".into(),
            base_asset: base.into(),
            quote_asset: quote.into(),
        }
    }

    fn exchange_info(symbols: Vec<SymbolInfo>) -> ExchangeInfo {
        ExchangeInfo {
            timezone: None,
            server_time: None,
            symbols,
        }
    }

    #[test]
    fn given_mixed_buy_sell_history_when_lots_from_trades_then_attributes_lifo_buys_until_balance_zero()
     {
        // Arrange: buy 10 @ 100, sell 5, buy 3 @ 120 → balance 8
        let balance_qty = 8.0;
        let history = vec![
            trade(1, "100", "10", true),
            trade(2, "110", "5", false),
            trade(3, "120", "3", true),
        ];

        // Act
        let lots = lots_from_trades(balance_qty, history, true);

        // Assert: newest buy 3 fully, then 5 from the older buy of 10
        assert_eq!(lots.len(), 2);
        assert!((lots[0].amount - 3.0).abs() < QTY_EPS);
        assert!((lots[0].entry_price - 120.0).abs() < QTY_EPS);
        assert!((lots[1].amount - 5.0).abs() < QTY_EPS);
        assert!((lots[1].entry_price - 100.0).abs() < QTY_EPS);
        let explained: f64 = lots.iter().map(|l| l.amount).sum();
        assert!((explained - balance_qty).abs() < QTY_EPS);
    }

    #[test]
    fn given_partial_last_lot_when_lots_from_trades_then_takes_only_remaining_qty()
     {
        // Arrange: single buy of 10, balance only 4
        let history = vec![trade(1, "50", "10", true)];

        // Act
        let lots = lots_from_trades(4.0, history, true);

        // Assert
        assert_eq!(lots.len(), 1);
        assert!((lots[0].amount - 4.0).abs() < QTY_EPS);
        assert!((lots[0].entry_price - 50.0).abs() < QTY_EPS);
    }

    #[test]
    fn given_only_sells_when_lots_from_trades_then_returns_no_lots() {
        // Arrange
        let history =
            vec![trade(1, "100", "1", false), trade(2, "101", "2", false)];

        // Act
        let lots = lots_from_trades(5.0, history, true);

        // Assert
        assert!(lots.is_empty());
    }

    #[test]
    fn given_quote_asset_holdings_when_lots_from_trades_then_uses_sells_and_inverted_price()
     {
        // Arrange: holding quote (e.g. BYN on USD/BYN). Selling base acquires quote.
        let history = vec![trade_with_quote(1, "2.0", "10", "20", false)];

        // Act
        let lots = lots_from_trades(20.0, history, false);

        // Assert: quote qty 20, entry_price = 1/2
        assert_eq!(lots.len(), 1);
        assert!((lots[0].amount - 20.0).abs() < QTY_EPS);
        assert!((lots[0].entry_price - 0.5).abs() < QTY_EPS);
    }

    #[test]
    fn given_truncated_history_when_lots_from_trades_then_explains_only_available_qty()
     {
        // Arrange: balance 100 but history only has one buy of 10
        let history = vec![trade(1, "10", "10", true)];

        // Act
        let lots = lots_from_trades(100.0, history, true);

        // Assert
        assert_eq!(lots.len(), 1);
        assert!((lots[0].amount - 10.0).abs() < QTY_EPS);
        let explained: f64 = lots.iter().map(|l| l.amount).sum();
        assert!(explained < 100.0);
    }

    #[test]
    fn given_usd_quoted_and_other_pairs_when_resolve_trade_pair_then_prefers_base_usd()
     {
        // Arrange
        let info = exchange_info(vec![
            symbol_info("BTC/EUR", "BTC", "EUR"),
            symbol_info("BTC/USD", "BTC", "USD"),
            symbol_info("ETH/BTC", "ETH", "BTC"),
        ]);

        // Act
        let resolved = resolve_trade_pair(&info, "BTC");

        // Assert
        assert_eq!(resolved, Some(("BTC/USD".to_string(), true)));
    }

    #[test]
    fn given_asset_only_as_quote_when_resolve_trade_pair_then_asset_is_not_base()
     {
        // Arrange
        let info = exchange_info(vec![symbol_info("USD/BYN", "USD", "BYN")]);

        // Act
        let resolved = resolve_trade_pair(&info, "BYN");

        // Assert
        assert_eq!(resolved, Some(("USD/BYN".to_string(), false)));
    }

    #[test]
    fn given_unknown_asset_when_resolve_trade_pair_then_returns_none() {
        // Arrange
        let info = exchange_info(vec![symbol_info("BTC/USD", "BTC", "USD")]);

        // Act
        let resolved = resolve_trade_pair(&info, "NOPE");

        // Assert
        assert!(resolved.is_none());
    }

    #[test]
    fn given_currency_map_when_resolve_asset_name_then_returns_display_name() {
        // Arrange
        let mut names = HashMap::new();
        names.insert("TSM".into(), "Taiwan Semiconductor".into());

        // Act
        let name = resolve_asset_name(&names, "TSM");

        // Assert
        assert_eq!(name.as_deref(), Some("Taiwan Semiconductor"));
    }

    #[test]
    fn given_leverage_symbol_when_resolve_asset_name_then_looks_up_base_ticker()
    {
        // Arrange
        let mut names = HashMap::new();
        names.insert("TSM".into(), "Taiwan Semiconductor".into());

        // Act
        let name = resolve_asset_name(&names, "TSM/USD_LEVERAGE");

        // Assert
        assert_eq!(name.as_deref(), Some("Taiwan Semiconductor"));
    }

    #[test]
    fn given_leverage_only_symbol_when_resolve_asset_name_then_returns_none() {
        // Arrange
        let names = HashMap::new();

        // Act
        let name = resolve_asset_name(&names, "FOO/USD_LEVERAGE");

        // Assert
        assert!(name.is_none());
    }

    #[test]
    fn given_symbol_with_trailing_dot_when_normalize_symbol_then_strips_dot() {
        // Arrange
        let raw = "TSM.";

        // Act
        let symbol = normalize_symbol(raw);

        // Assert
        assert_eq!(symbol, "TSM");
    }

    #[test]
    fn given_leverage_pair_when_lookup_symbol_then_returns_base_ticker() {
        // Arrange
        let raw = "TSM/USD_LEVERAGE";

        // Act
        let key = lookup_symbol(raw);

        // Assert
        assert_eq!(key, "TSM");
    }

    #[test]
    fn given_index_cfd_when_lookup_symbol_then_maps_to_yahoo_index() {
        assert_eq!(lookup_symbol("US500"), "^GSPC");
        assert_eq!(lookup_symbol("US100"), "^NDX");
        assert_eq!(lookup_symbol("US30"), "^DJI");
    }
}
