use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::{
    AccountInformation, Currency, TradingPosition,
    investment::{Balance, ExchangeInfo, RestClient, Trade},
};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Equity,
    Index,
    Commodity,
    Crypto,
    Fx,
    Other,
}

/// Wallet + book snapshot for MCP `trading_portfolio`.
///
/// Cash fields come from the Dzengi USD balance row. Position notionals and
/// per-lot broker margin come from `/tradingPositions` and are **not** folded
/// into `nav`. Prefer `reserved_cash` for total locked wallet cash; do **not**
/// treat `margin_used` as total locked margin (it can be much smaller).
///
/// Legacy `current_volume` was removed on purpose (use `cash` / `reserved_cash`
/// / `nav` / `historical_volume` instead).
#[derive(Clone, serde::Serialize, schemars::JsonSchema)]
pub struct Portfolio {
    pub snapshot_at: DateTime<Utc>,
    pub currency: String,
    /// USD wallet `free` (available). `None` if no USD balance row.
    pub cash: Option<f64>,
    /// USD wallet `locked` (broker-reserved cash). With `cash`, this is the
    /// Equity side of the wallet: `nav = cash + reserved_cash`. Independent of
    /// `margin_used` and of CFD notionals in `positions_value`.
    pub reserved_cash: Option<f64>,
    /// Σ open holding `market_value` (spot marks + leverage `cost` marks).
    /// CFD notionals can dwarf `nav`; not added into `nav`.
    pub positions_value: f64,
    /// Wallet Equity: `cash + reserved_cash` when both known. Never
    /// `cash + positions_value`.
    pub nav: Option<f64>,
    /// Σ open holding unrealized P/L.
    pub unrealized_pnl: f64,
    /// Always `null` today (P1): not yet wired from ledger / closed-position
    /// history. Do not infer realized P/L from other fields.
    pub realized_pnl: Option<f64>,
    /// Same as `cash` (USD free). Not a margin-adjusted figure.
    pub buying_power: Option<f64>,
    /// Σ Dzengi `TradingPosition.margin` for **all** open long lots, scaled by
    /// remaining qty (`(open-close)/open`). Aggregated across every leveraged
    /// product — not a single-name copy. Distinct from `reserved_cash` (wallet
    /// `locked`); live books often show `margin_used` << `reserved_cash`. Use
    /// `reserved_cash` for total locked cash; use this only as the sum of
    /// broker per-position margin fields. Shorts are skipped.
    pub margin_used: Option<f64>,

    pub can_trade: bool,
    pub can_withdraw: bool,
    pub can_deposit: bool,

    /// Lifetime deposit volume from ledger (USD-normalized), not live book size.
    /// Replaces legacy `current_volume` for funding history.
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
    pub asset_class: AssetClass,
    /// True when `/exchangeInfo` marks the name as leveraged/CFD (not spot).
    /// Portfolio `margin_used` is the Σ of broker per-lot `margin` for open
    /// longs; individual lot margin is not exposed on this asset row.
    pub leverage: bool,

    pub amount: f64,
    pub average_entry_price: f64,
    /// Lot-reconstructed entry notional (`sum(entry_price * amount)`), not broker `cost`.
    pub cost_basis: f64,
    /// Broker mark. Same source as `market_value`.
    pub unit_market_price: f64,
    /// Live market value of the holding (`unit_market_price * amount`).
    pub market_value: f64,
    pub unrealized_pnl: f64,
    /// Percent, not fraction.
    pub unrealized_pnl_pct: f64,
    pub currency: String,

    /// Lot list; omitted from lean `trading_positions` unless `include_trades`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trades: Vec<AssetEntryTrade>,
}

pub struct DzengiSnapshot {
    pub snapshot_at: DateTime<Utc>,
    pub account: AccountInformation,
    pub positions: Vec<TradingPosition>,
    pub currencies: Vec<Currency>,
    pub exchange_info: ExchangeInfo,
}

pub struct Holdings {
    pub assets: Vec<Asset>,
    /// Σ scaled open-long `TradingPosition.margin`; see [`Portfolio::margin_used`].
    pub margin_used: f64,
}

pub async fn load_dzengi_snapshot(
    api: &RestClient,
) -> anyhow::Result<DzengiSnapshot> {
    let server_ts = api.time().await?;
    let (account, positions, currencies, exchange_info) = tokio::try_join!(
        api.account(server_ts),
        api.trading_positions(server_ts),
        api.currencies(server_ts),
        api.exchange_info(server_ts),
    )?;

    Ok(DzengiSnapshot {
        snapshot_at: Utc::now(),
        account,
        positions,
        currencies,
        exchange_info,
    })
}

pub fn usd_wallet(account: &AccountInformation) -> (Option<f64>, Option<f64>) {
    match account.balances.iter().find(|b| b.asset == "USD") {
        Some(b) => (Some(b.free), Some(b.locked)),
        None => (None, None),
    }
}

pub fn nav_from_wallet(
    cash: Option<f64>,
    reserved_cash: Option<f64>,
) -> Option<f64> {
    match (cash, reserved_cash) {
        (Some(c), Some(r)) => Some(c + r),
        _ => None,
    }
}

pub async fn fetch_portfolio(api: &RestClient) -> anyhow::Result<Portfolio> {
    let snapshot = load_dzengi_snapshot(api).await?;
    let holdings = assemble_holdings(api, &snapshot).await?;

    let (cash, reserved_cash) = usd_wallet(&snapshot.account);
    if let Some(c) = cash {
        anyhow::ensure!(c >= 0.0, "cash is negative: {c}");
    }
    let nav = nav_from_wallet(cash, reserved_cash);
    if let Some(n) = nav {
        anyhow::ensure!(n >= 0.0, "nav is negative: {n}");
    }

    let positions_value: f64 =
        holdings.assets.iter().map(|a| a.market_value).sum();
    let unrealized_pnl: f64 =
        holdings.assets.iter().map(|a| a.unrealized_pnl).sum();

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

    Ok(Portfolio {
        snapshot_at: snapshot.snapshot_at,
        currency: "USD".into(),
        cash,
        reserved_cash,
        positions_value,
        nav,
        unrealized_pnl,
        realized_pnl: None,
        buying_power: cash,
        margin_used: Some(holdings.margin_used),
        historical_volume,
        total_withdrawal,
        total_fee_spending: total_fee,
        can_trade: snapshot.account.can_trade,
        can_withdraw: snapshot.account.can_withdraw,
        can_deposit: snapshot.account.can_deposit,
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
    let s = s.strip_suffix("/USD").unwrap_or(s);

    // Broker CFD / index names → Yahoo-style symbols (technicals / news).
    // Analyst price targets still often missing for pure indices.
    // Collisions to avoid: GOLD (Barrick path), bare GSPC, TON-USD (junk token).
    match s {
        "US500" | "SPX" | "SP500" | "SPX500" => "^GSPC".into(),
        "US100" | "NDX" | "NAS100" | "USTEC" => "^NDX".into(),
        "US30" | "DJIA" | "DOW" | "WALLSTREET30" => "^DJI".into(),
        "DE40" | "DAX" | "GER40" => "^GDAXI".into(),
        "UK100" | "FTSE" | "UK100GBP" => "^FTSE".into(),
        "JP225" | "NI225" | "NIKKEI" => "^N225".into(),
        "Gold" | "XAU" | "XAUm" | "XAUUSD" => "GC=F".into(),
        // Yahoo `TON-USD` is a different ERC-20 (~0.005). Toncoin is TON11419-USD.
        "TON" | "TONCOIN" => "TON11419-USD".into(),
        other => other.to_string(),
    }
}

pub fn asset_class_from_dzengi(asset_type: &str) -> Option<AssetClass> {
    match asset_type.to_ascii_uppercase().as_str() {
        "EQUITY" => Some(AssetClass::Equity),
        "INDEX" => Some(AssetClass::Index),
        "COMMODITY" => Some(AssetClass::Commodity),
        "CRYPTOCURRENCY" | "ICO" | "OPT_TOKENS" | "UTILITY_TOKENS" => {
            Some(AssetClass::Crypto)
        }
        "CURRENCY" => Some(AssetClass::Fx),
        "BOND" | "CREDIT" | "INTEREST_RATE" | "REAL_ESTATE" | "OTHER_ASSET" => {
            Some(AssetClass::Other)
        }
        _ => None,
    }
}

pub fn asset_class_fallback(symbol: &str) -> AssetClass {
    let s = normalize_symbol(symbol);
    let s = s.strip_suffix("/USD_LEVERAGE").unwrap_or(&s);
    match s {
        "US500" | "SPX" | "SP500" | "SPX500" | "US100" | "NDX" | "NAS100"
        | "USTEC" | "US30" | "DJIA" | "DOW" | "WALLSTREET30" | "DE40"
        | "DAX" | "GER40" | "UK100" | "FTSE" | "UK100GBP" | "JP225"
        | "NI225" | "NIKKEI" => AssetClass::Index,
        "Gold" | "Silver" | "XAU" | "XAUm" | "XAG" | "XAGm" | "XTI" | "XBR"
        | "XNG" => AssetClass::Commodity,
        "TON" | "BTC" | "ETH" | "BCH" | "LTC" | "XRP" => AssetClass::Crypto,
        "WMT" | "TSLA" | "IBM" | "GOOGL" | "SNE" | "SONY" | "TSM" => {
            AssetClass::Equity
        }
        _ => AssetClass::Other,
    }
}

fn find_symbol_info<'a>(
    exchange_info: &'a ExchangeInfo,
    symbol: &str,
) -> Option<&'a crate::investment::SymbolInfo> {
    let normalized = normalize_symbol(symbol);
    exchange_info
        .symbols
        .iter()
        .find(|s| s.symbol == symbol)
        .or_else(|| {
            exchange_info
                .symbols
                .iter()
                .find(|s| s.symbol == normalized)
        })
}

pub fn classify_asset(
    symbol: &str,
    exchange_info: &ExchangeInfo,
) -> (AssetClass, bool) {
    let info = find_symbol_info(exchange_info, symbol);
    let leverage = info
        .and_then(|s| s.market_type.as_deref())
        .is_some_and(|t| t.eq_ignore_ascii_case("LEVERAGE"))
        || symbol.contains("_LEVERAGE");

    let class = info
        .and_then(|s| s.asset_type.as_deref())
        .and_then(asset_class_from_dzengi)
        .unwrap_or_else(|| asset_class_fallback(symbol));

    (class, leverage)
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

    let market_value = amount * unit_market_price;
    let unrealized_pnl = market_value - entry_cost;
    let unrealized_pnl_pct = if entry_cost.abs() > f64::EPSILON {
        unrealized_pnl / entry_cost * 100.0
    } else {
        0.0
    };
    let (asset_class, leverage) = classify_asset(&symbol, exchange_info);

    Ok(Some(Asset {
        name: resolve_asset_name(name_by_symbol, &symbol),
        symbol,
        asset_class,
        leverage,
        amount,
        average_entry_price,
        cost_basis: entry_cost,
        unit_market_price,
        market_value,
        unrealized_pnl,
        unrealized_pnl_pct,
        currency: "USD".into(),
        trades,
    }))
}

fn build_leverage_assets(
    positions: Vec<TradingPosition>,
    name_by_symbol: &HashMap<String, String>,
    exchange_info: &ExchangeInfo,
) -> (Vec<Asset>, f64) {
    let mut assets_by_symbol = HashMap::<String, Asset>::new();
    let mut margin_used = 0.0;

    for position in positions {
        // Live size is opened minus realized (e.g. opened 2, closed 1 → 1).
        if position.open_qty < -QTY_EPS {
            tracing::warn!(
                symbol = %position.symbol,
                id = %position.id,
                open_qty = position.open_qty,
                close_qty = position.close_qty,
                "Short operations are not supported yet. Ignore it"
            );
            continue;
        }

        let remaining_qty = position.open_qty - position.close_qty;

        let symbol = normalize_symbol(&position.symbol);
        let name = resolve_asset_name(name_by_symbol, &symbol);
        let (asset_class, leverage) = classify_asset(&symbol, exchange_info);
        let currency = if position.currency.is_empty() {
            "USD".to_string()
        } else {
            position.currency.clone()
        };

        if remaining_qty <= QTY_EPS {
            if position.open_qty.abs() <= QTY_EPS
                && position.close_qty.abs() <= QTY_EPS
            {
                tracing::warn!(
                    "Too small owning assets: {} count={}",
                    position.symbol,
                    remaining_qty
                );
            }
            continue;
        }

        // Exchange cost/upl are for opened size; scale to leftover qty.
        // TODO: use closed positions for historical performance review
        let remaining_frac = remaining_qty / position.open_qty;
        let remaining_market_value = position.cost * remaining_frac;
        let remaining_pl = position.profit_loss * remaining_frac;
        // Aggregate broker per-lot margin across every open long product.
        // Not wallet `reserved_cash` (USD locked); may be much smaller.
        margin_used += position.margin * remaining_frac;

        assets_by_symbol
            .entry(symbol.clone())
            .and_modify(|a| {
                a.unrealized_pnl += remaining_pl;
                a.market_value += remaining_market_value;
                a.amount += remaining_qty;

                a.trades.push(AssetEntryTrade {
                    entry_price: position.open_price,
                    amount: remaining_qty,
                });
            })
            .or_insert(Asset {
                symbol,
                name,
                asset_class,
                leverage,
                amount: remaining_qty,
                average_entry_price: 0.0,
                cost_basis: 0.0,
                unit_market_price: 0.0,
                market_value: remaining_market_value,
                unrealized_pnl: remaining_pl,
                unrealized_pnl_pct: 0.0,
                currency,
                trades: vec![AssetEntryTrade {
                    amount: remaining_qty,
                    entry_price: position.open_price,
                }],
            });
    }

    let assets = assets_by_symbol
        .into_values()
        .map(|mut a| {
            assert!(!a.trades.is_empty());

            let entry_cost: f64 =
                a.trades.iter().map(|t| t.entry_price * t.amount).sum();

            assert!(a.amount.abs() > QTY_EPS);

            a.cost_basis = entry_cost;
            a.average_entry_price = entry_cost / a.amount;
            a.unit_market_price = a.market_value / a.amount;
            a.unrealized_pnl_pct = if entry_cost.abs() > f64::EPSILON {
                a.unrealized_pnl / entry_cost * 100.0
            } else {
                0.0
            };
            a
        })
        .collect::<Vec<_>>();

    (assets, margin_used)
}

pub async fn assemble_holdings(
    api_client: &RestClient,
    snapshot: &DzengiSnapshot,
) -> anyhow::Result<Holdings> {
    let server_ts = api_client.time().await?;

    let name_by_symbol: HashMap<String, String> = snapshot
        .currencies
        .iter()
        .map(|c| (c.symbol.clone(), c.name.clone()))
        .collect();

    let mut spot_assets = Vec::new();

    for balance in &snapshot.account.balances {
        let Some(asset) = spot_asset_from_balance(
            api_client,
            balance,
            &snapshot.exchange_info,
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

    let (leverage_assets, margin_used) = build_leverage_assets(
        snapshot.positions.clone(),
        &name_by_symbol,
        &snapshot.exchange_info,
    );

    Ok(Holdings {
        assets: [spot_assets, leverage_assets].concat(),
        margin_used,
    })
}

/// Spot balances + leverage positions as raw holdings (no external analysis).
pub async fn fetch_holding_assets(
    api_client: &RestClient,
) -> anyhow::Result<Vec<Asset>> {
    let snapshot = load_dzengi_snapshot(api_client).await?;
    Ok(assemble_holdings(api_client, &snapshot).await?.assets)
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
            asset_type: None,
            market_type: None,
        }
    }

    fn symbol_info_typed(
        symbol: &str,
        base: &str,
        quote: &str,
        asset_type: &str,
        market_type: &str,
    ) -> SymbolInfo {
        SymbolInfo {
            symbol: symbol.into(),
            status: "TRADING".into(),
            base_asset: base.into(),
            quote_asset: quote.into(),
            asset_type: Some(asset_type.into()),
            market_type: Some(market_type.into()),
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

    #[allow(clippy::too_many_arguments)]
    fn leverage_position(
        id: &str,
        symbol: &str,
        open_qty: f64,
        close_qty: f64,
        open_price: f64,
        close_price: f64,
        cost: f64,
        profit_loss: f64,
    ) -> TradingPosition {
        TradingPosition {
            symbol: symbol.into(),
            id: id.into(),
            account_id: "acct".into(),
            margin: 0.0,
            fee: 0.0,
            open_qty,
            close_qty,
            open_price,
            close_price,
            cost,
            profit_loss,
            currency: "USD".into(),
            created_at: chrono::DateTime::from_timestamp_millis(0).unwrap(),
        }
    }

    #[test]
    fn given_one_of_two_lots_closed_when_build_leverage_assets_then_keeps_remaining_open_minus_close()
     {
        // Arrange: two lots of 1; one fully closed (open == close), the other still open
        let positions = vec![
            leverage_position(
                "open",
                "TSM/USD_LEVERAGE",
                1.0,
                0.0,
                100.0,
                0.0,
                110.0,
                10.0,
            ),
            leverage_position(
                "closed",
                "TSM/USD_LEVERAGE",
                1.0,
                1.0,
                100.0,
                105.0,
                0.0,
                0.0,
            ),
        ];

        // Act
        let (assets, margin) = build_leverage_assets(
            positions,
            &HashMap::new(),
            &exchange_info(vec![]),
        );

        // Assert
        assert_eq!(assets.len(), 1);
        assert!((assets[0].amount - 1.0).abs() < QTY_EPS);
        assert!((assets[0].market_value - 110.0).abs() < QTY_EPS);
        assert!((assets[0].unrealized_pnl - 10.0).abs() < QTY_EPS);
        assert_eq!(assets[0].trades.len(), 1);
        assert!((margin - 0.0).abs() < QTY_EPS);
    }

    #[test]
    fn given_partially_closed_position_when_build_leverage_assets_then_amount_is_open_minus_close()
     {
        // Arrange: opened 2, closed 1 → live size 1
        let positions = vec![leverage_position(
            "partial",
            "TSM/USD_LEVERAGE",
            2.0,
            1.0,
            100.0,
            105.0,
            110.0,
            10.0,
        )];

        // Act
        let (assets, _) = build_leverage_assets(
            positions,
            &HashMap::new(),
            &exchange_info(vec![]),
        );

        // Assert: leftover is half of opened size, so cost/upl scale 110→55, 10→5
        assert_eq!(assets.len(), 1);
        assert!((assets[0].amount - 1.0).abs() < QTY_EPS);
        assert!((assets[0].market_value - 55.0).abs() < QTY_EPS);
        assert!((assets[0].unrealized_pnl - 5.0).abs() < QTY_EPS);
        assert!((assets[0].cost_basis - 100.0).abs() < QTY_EPS);
        assert!((assets[0].average_entry_price - 100.0).abs() < QTY_EPS);
        assert!((assets[0].unit_market_price - 55.0).abs() < QTY_EPS);
        assert_eq!(assets[0].trades[0].amount, 1.0);
    }

    #[test]
    fn given_fully_closed_position_when_build_leverage_assets_then_skips_it() {
        // Arrange: opened 2, closed 2
        let positions = vec![leverage_position(
            "closed",
            "TSM/USD_LEVERAGE",
            2.0,
            2.0,
            100.0,
            105.0,
            0.0,
            0.0,
        )];

        // Act
        let (assets, margin) = build_leverage_assets(
            positions,
            &HashMap::new(),
            &exchange_info(vec![]),
        );

        // Assert
        assert!(assets.is_empty());
        assert!((margin - 0.0).abs() < QTY_EPS);
    }

    #[test]
    fn given_short_position_when_build_leverage_assets_then_skips_it() {
        // Arrange
        let positions = vec![leverage_position(
            "short",
            "TSM/USD_LEVERAGE",
            -2.0,
            0.0,
            100.0,
            0.0,
            200.0,
            5.0,
        )];

        // Act
        let (assets, margin) = build_leverage_assets(
            positions,
            &HashMap::new(),
            &exchange_info(vec![]),
        );

        // Assert
        assert!(assets.is_empty());
        assert!((margin - 0.0).abs() < QTY_EPS);
    }

    fn account_with_usd(free: f64, locked: f64) -> AccountInformation {
        AccountInformation {
            maker_commission: None,
            taker_commission: None,
            can_trade: true,
            can_withdraw: true,
            can_deposit: true,
            update_time: None,
            balances: vec![crate::investment::Balance {
                asset: "USD".into(),
                free,
                locked,
                timestamp: None,
            }],
        }
    }

    #[test]
    fn given_usd_wallet_when_nav_from_wallet_then_is_free_plus_locked_not_positions()
     {
        let (cash, reserved) = usd_wallet(&account_with_usd(100.0, 20.0));
        let nav = nav_from_wallet(cash, reserved);
        assert_eq!(cash, Some(100.0));
        assert_eq!(reserved, Some(20.0));
        assert_eq!(nav, Some(120.0));
        let positions_value = 5000.0;
        assert_ne!(nav, Some(cash.unwrap() + positions_value));
    }

    #[test]
    fn given_no_usd_row_when_usd_wallet_then_fields_are_none() {
        let account = AccountInformation {
            maker_commission: None,
            taker_commission: None,
            can_trade: true,
            can_withdraw: true,
            can_deposit: true,
            update_time: None,
            balances: vec![],
        };
        let (cash, reserved) = usd_wallet(&account);
        assert!(cash.is_none());
        assert!(reserved.is_none());
        assert!(nav_from_wallet(cash, reserved).is_none());
    }

    #[test]
    fn given_index_cfd_when_classify_asset_then_uses_exchange_info() {
        let info = exchange_info(vec![symbol_info_typed(
            "US500", "US500", "USD", "INDEX", "LEVERAGE",
        )]);
        let (class, leverage) = classify_asset("US500", &info);
        assert_eq!(class, AssetClass::Index);
        assert!(leverage);
    }

    #[test]
    fn given_missing_asset_type_when_classify_gold_then_falls_back_to_commodity()
     {
        let (class, leverage) = classify_asset("Gold", &exchange_info(vec![]));
        assert_eq!(class, AssetClass::Commodity);
        assert!(!leverage);
    }

    #[test]
    fn given_gold_when_lookup_symbol_then_maps_to_gc_futures() {
        assert_eq!(lookup_symbol("Gold"), "GC=F");
        assert_eq!(lookup_symbol("XAUUSD"), "GC=F");
        assert_ne!(lookup_symbol("Gold"), "GOLD");
    }

    #[test]
    fn given_ton_variants_when_lookup_symbol_then_maps_to_toncoin_yahoo_id() {
        assert_eq!(lookup_symbol("TON"), "TON11419-USD");
        assert_eq!(lookup_symbol("TON/USD_LEVERAGE"), "TON11419-USD");
        assert_eq!(lookup_symbol("TON/USD"), "TON11419-USD");
        assert_eq!(lookup_symbol("TONCOIN"), "TON11419-USD");
        assert_ne!(lookup_symbol("TON"), "TON-USD");
    }

    #[test]
    fn given_us500_when_lookup_symbol_then_maps_to_caret_gspc_not_bare_gspc() {
        assert_eq!(lookup_symbol("US500"), "^GSPC");
        assert_ne!(lookup_symbol("US500"), "GSPC");
        assert_ne!(lookup_symbol("US500"), "SPY");
    }

    #[test]
    fn given_ton_leverage_when_classify_then_crypto_and_levered() {
        let (class, leverage) =
            classify_asset("TON/USD_LEVERAGE", &exchange_info(vec![]));
        assert_eq!(class, AssetClass::Crypto);
        assert!(leverage);
    }

    #[test]
    fn given_multiple_open_lots_when_build_leverage_assets_then_margin_sums_all_products()
     {
        // Arrange: margins on three products; one lot half-closed (frac 0.5)
        let mut tsm = leverage_position(
            "tsm",
            "TSM/USD_LEVERAGE",
            1.0,
            0.0,
            100.0,
            0.0,
            110.0,
            10.0,
        );
        tsm.margin = 10.0;
        let mut aapl = leverage_position(
            "aapl",
            "AAPL/USD_LEVERAGE",
            2.0,
            0.0,
            50.0,
            0.0,
            200.0,
            5.0,
        );
        aapl.margin = 20.0;
        let mut us500 = leverage_position(
            "us500", "US500", 2.0, 1.0, 4000.0, 0.0, 8000.0, 100.0,
        );
        us500.margin = 30.0; // remaining_frac = 0.5 → contributes 15

        // Act
        let (assets, margin) = build_leverage_assets(
            vec![tsm, aapl, us500],
            &HashMap::new(),
            &exchange_info(vec![]),
        );

        // Assert: 10 + 20 + 15 = 45 — full cross-product aggregation, not one name
        assert_eq!(assets.len(), 3);
        assert!((margin - 45.0).abs() < QTY_EPS);
    }

    #[test]
    fn given_portfolio_when_serialized_then_has_snapshot_and_no_legacy_volume()
    {
        let p = Portfolio {
            snapshot_at: Utc::now(),
            currency: "USD".into(),
            cash: Some(100.0),
            reserved_cash: Some(20.0),
            positions_value: 5000.0,
            nav: Some(120.0),
            unrealized_pnl: -10.0,
            realized_pnl: None,
            buying_power: Some(100.0),
            margin_used: Some(50.0),
            can_trade: true,
            can_withdraw: true,
            can_deposit: true,
            historical_volume: 1.0,
            total_fee_spending: 0.0,
            total_withdrawal: 0.0,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert!(v.get("snapshot_at").is_some());
        assert!(v.get("cash").is_some());
        assert!(v.get("nav").is_some());
        assert!(v.get("reserved_cash").is_some());
        assert!(v.get("margin_used").is_some());
        // realized_pnl present but null until ledger wiring (P1)
        assert!(v.get("realized_pnl").is_some());
        assert!(v.get("realized_pnl").unwrap().is_null());
        // current_volume intentionally dropped in trading snapshot (#5 / #8)
        assert!(v.get("current_volume").is_none());
        assert!(v.get("as_of").is_none());
    }
}
