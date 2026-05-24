use std::sync::Arc;

use rmcp::{ServerHandler, model::ServerInfo, tool};
use serde::Deserialize;

use finance::portfolio::RestClient;

use crate::tools::{account, exchange, ledger, ticker, trading};

/// MCP server that exposes the DZENGI.com REST API as tools.
#[derive(Clone)]
pub struct FinanceMcpServer {
    pub api: Arc<RestClient>,
}

#[tool(tool_box)]
impl FinanceMcpServer {
    // ── Exchange ───────────────────────────────────────────────────────────────

    /// Returns the current server time from the DZENGI exchange as a Unix
    /// millisecond timestamp.
    #[tool(description = "Get the current DZENGI.com server time (Unix ms)")]
    async fn get_server_time(&self) -> String {
        match exchange::get_server_time(&self.api).await {
            Ok(ts)   => ts.to_string(),
            Err(e)   => format!("error: {e}"),
        }
    }

    /// Returns the list of all currencies supported by the exchange.
    #[tool(description = "List all currencies supported by DZENGI.com exchange")]
    async fn get_currencies(&self) -> String {
        match exchange::get_currencies(&self.api).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    /// Returns the current order book (bids and asks) for a trading pair.
    #[tool(description = "Get the order book (bids/asks) for a symbol, e.g. BTC/USD")]
    async fn get_order_book(
        &self,
        #[tool(description = "Trading pair symbol, e.g. \"BTC/USD\"")] symbol: String,
    ) -> String {
        match exchange::get_order_book(&self.api, &symbol).await {
            Ok(ob) => serde_json::to_string_pretty(&ob).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    /// Returns exchange metadata: all available trading pairs with their base
    /// and quote assets.
    #[tool(description = "Get exchange metadata: all trading pairs, base/quote assets, status")]
    async fn get_exchange_info(&self) -> String {
        match exchange::get_exchange_info(&self.api).await {
            Ok(info) => serde_json::to_string_pretty(&info).unwrap_or_else(|e| format!("error: {e}")),
            Err(e)   => format!("error: {e}"),
        }
    }

    /// Returns OHLCV candlestick data for a symbol and interval.
    #[tool(description = "Get OHLCV candlestick data for a symbol. interval examples: 1m, 5m, 15m, 1h, 4h, 1d")]
    async fn get_klines(
        &self,
        #[tool(description = "Trading pair symbol, e.g. \"BTC/USD\"")] symbol: String,
        #[tool(description = "Candlestick interval, e.g. \"1h\", \"1d\"")] interval: String,
    ) -> String {
        match exchange::get_klines(&self.api, &symbol, &interval).await {
            Ok(klines) => serde_json::to_string_pretty(&klines).unwrap_or_else(|e| format!("error: {e}")),
            Err(e)     => format!("error: {e}"),
        }
    }

    // ── Account ────────────────────────────────────────────────────────────────

    /// Returns account information including all asset balances.
    #[tool(description = "Get account information: balances, permissions, commission rates")]
    async fn get_account_balances(&self) -> String {
        match account::get_account(&self.api).await {
            Ok(acc) => serde_json::to_string_pretty(&acc).unwrap_or_else(|e| format!("error: {e}")),
            Err(e)  => format!("error: {e}"),
        }
    }

    /// Returns the deposit history for the account.
    #[tool(description = "Get deposit history for the account")]
    async fn get_deposits(&self) -> String {
        match account::get_deposits(&self.api).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    /// Returns trade history for the given symbol.
    #[tool(description = "Get trade history for a symbol, e.g. BTC/USD")]
    async fn get_my_trades(
        &self,
        #[tool(description = "Trading pair symbol, e.g. \"BTC/USD\"")] symbol: String,
    ) -> String {
        match account::get_my_trades(&self.api, &symbol).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    /// Returns details of a single order by its ID.
    #[tool(description = "Fetch a specific order by its order ID")]
    async fn fetch_order(
        &self,
        #[tool(description = "Order ID string")] order_id: String,
    ) -> String {
        match account::fetch_order(&self.api, &order_id).await {
            Ok(o)  => serde_json::to_string_pretty(&o).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    /// Returns all open orders. Optionally filter by symbol.
    #[tool(description = "Get open orders. Leave symbol empty to fetch all open orders")]
    async fn get_open_orders(
        &self,
        #[tool(description = "Optional trading pair symbol filter. Pass empty string for all")] symbol: String,
    ) -> String {
        let sym = if symbol.is_empty() { None } else { Some(symbol.as_str()) };
        match account::get_open_orders(&self.api, sym).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    // ── Ledger ─────────────────────────────────────────────────────────────────

    /// Returns the full paginated ledger for a currency.
    #[tool(description = "Get the full ledger (all entries) for a currency. Leave currency empty for all currencies")]
    async fn get_ledger(
        &self,
        #[tool(description = "Currency name e.g. \"BTC\". Pass empty string for all")] currency: String,
    ) -> String {
        let cur = if currency.is_empty() { None } else { Some(currency.as_str()) };
        match ledger::get_full_ledger(&self.api, cur).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    /// Returns all deposit and withdrawal transactions (auto-paginated).
    #[tool(description = "Get all deposit and withdrawal transactions (full history, paginated automatically)")]
    async fn get_transactions(&self) -> String {
        match ledger::get_all_transactions(&self.api).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    // ── Trading positions ─────────────────────────────────────────────────────

    /// Returns currently open trading (CFD/margin) positions.
    #[tool(description = "Get currently open trading (CFD/margin) positions")]
    async fn get_trading_positions(&self) -> String {
        match trading::get_trading_positions(&self.api).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    /// Returns closed trading position history. Optionally filter by symbol.
    #[tool(description = "Get closed trading position history. Leave symbol empty for all symbols")]
    async fn get_trading_position_history(
        &self,
        #[tool(description = "Optional symbol filter. Pass empty string for all")] symbol: String,
    ) -> String {
        let sym = if symbol.is_empty() { None } else { Some(symbol.as_str()) };
        match trading::get_trading_position_history(&self.api, sym).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    // ── Ticker ─────────────────────────────────────────────────────────────────

    /// Returns 24-hour price statistics for a symbol.
    #[tool(description = "Get 24-hour price statistics (OHLC, volume, change%) for a symbol")]
    async fn get_ticker(
        &self,
        #[tool(description = "Trading pair symbol, e.g. \"BTC/USD\"")] symbol: String,
    ) -> String {
        match ticker::get_ticker(&self.api, &symbol).await {
            Ok(t)  => serde_json::to_string_pretty(&t).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }
}

#[rmcp::async_trait]
impl ServerHandler for FinanceMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            name: "finance-mcp".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            ..Default::default()
        }
    }
}
