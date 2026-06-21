use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerInfo},
    tool, tool_handler, tool_router,
};
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use finance::portfolio::RestClient;

use crate::tools::{account, exchange, ledger, ticker, trading};

// ── Parameter structs ─────────────────────────────────────────────────────────

/// Parameters for tools that accept a trading symbol.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SymbolParams {
    /// Trading pair symbol, e.g. "BTC/USD"
    pub symbol: String,
}

/// Parameters for get_klines.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct KlinesParams {
    /// Trading pair symbol, e.g. "BTC/USD"
    pub symbol: String,
    /// Candlestick interval, e.g. "1h", "1d"
    pub interval: String,
}

/// Parameters for tools that accept an optional symbol filter.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionalSymbolParams {
    /// Optional trading pair symbol filter. Pass empty string for all.
    pub symbol: String,
}

/// Parameters for fetch_order.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct OrderIdParams {
    /// Order ID string
    pub order_id: String,
}

/// Parameters for get_ledger.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CurrencyParams {
    /// Currency name e.g. "BTC". Pass empty string for all.
    pub currency: String,
}

// ── MCP server ────────────────────────────────────────────────────────────────

/// MCP server that exposes the DZENGI.com REST API as tools.
#[derive(Clone)]
pub struct FinanceMcpServer {
    pub api: Arc<RestClient>,
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl FinanceMcpServer {
    pub fn new(api: Arc<RestClient>) -> Self {
        Self {
            api,
            tool_router: Self::tool_router(),
        }
    }

    // ── Exchange ───────────────────────────────────────────────────────────────

    #[tool(description = "Get the current DZENGI.com server time (Unix ms)")]
    async fn get_server_time(&self) -> String {
        match exchange::get_server_time(&self.api).await {
            Ok(ts)   => ts.to_string(),
            Err(e)   => format!("error: {e}"),
        }
    }

    #[tool(description = "List all currencies supported by DZENGI.com exchange")]
    async fn get_currencies(&self) -> String {
        match exchange::get_currencies(&self.api).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Get the order book (bids/asks) for a symbol, e.g. BTC/USD")]
    async fn get_order_book(&self, Parameters(p): Parameters<SymbolParams>) -> String {
        match exchange::get_order_book(&self.api, &p.symbol).await {
            Ok(ob) => serde_json::to_string_pretty(&ob).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Get exchange metadata: all trading pairs, base/quote assets, status")]
    async fn get_exchange_info(&self) -> String {
        match exchange::get_exchange_info(&self.api).await {
            Ok(info) => serde_json::to_string_pretty(&info).unwrap_or_else(|e| format!("error: {e}")),
            Err(e)   => format!("error: {e}"),
        }
    }

    #[tool(description = "Get OHLCV candlestick data for a symbol. interval examples: 1m, 5m, 15m, 1h, 4h, 1d")]
    async fn get_klines(&self, Parameters(p): Parameters<KlinesParams>) -> String {
        match exchange::get_klines(&self.api, &p.symbol, &p.interval).await {
            Ok(klines) => serde_json::to_string_pretty(&klines).unwrap_or_else(|e| format!("error: {e}")),
            Err(e)     => format!("error: {e}"),
        }
    }

    // ── Account ────────────────────────────────────────────────────────────────

    #[tool(description = "Get account information: balances, permissions, commission rates")]
    async fn get_account_balances(&self) -> String {
        match account::get_account(&self.api).await {
            Ok(acc) => serde_json::to_string_pretty(&acc).unwrap_or_else(|e| format!("error: {e}")),
            Err(e)  => format!("error: {e}"),
        }
    }

    #[tool(description = "Get deposit history for the account")]
    async fn get_deposits(&self) -> String {
        match account::get_deposits(&self.api).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Get trade history for a symbol, e.g. BTC/USD")]
    async fn get_my_trades(&self, Parameters(p): Parameters<SymbolParams>) -> String {
        match account::get_my_trades(&self.api, &p.symbol).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Fetch a specific order by its order ID")]
    async fn fetch_order(&self, Parameters(p): Parameters<OrderIdParams>) -> String {
        match account::fetch_order(&self.api, &p.order_id).await {
            Ok(o)  => serde_json::to_string_pretty(&o).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Get open orders. Leave symbol empty to fetch all open orders")]
    async fn get_open_orders(&self, Parameters(p): Parameters<OptionalSymbolParams>) -> String {
        let sym = if p.symbol.is_empty() { None } else { Some(p.symbol.as_str()) };
        match account::get_open_orders(&self.api, sym).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    // ── Ledger ─────────────────────────────────────────────────────────────────

    #[tool(description = "Get the full ledger (all entries) for a currency. Leave currency empty for all currencies")]
    async fn get_ledger(&self, Parameters(p): Parameters<CurrencyParams>) -> String {
        let cur = if p.currency.is_empty() { None } else { Some(p.currency.as_str()) };
        match ledger::get_full_ledger(&self.api, cur).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Get all deposit and withdrawal transactions (full history, paginated automatically)")]
    async fn get_transactions(&self) -> String {
        match ledger::get_all_transactions(&self.api).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    // ── Trading positions ─────────────────────────────────────────────────────

    #[tool(description = "Get currently open trading (CFD/margin) positions")]
    async fn get_trading_positions(&self) -> String {
        match trading::get_trading_positions(&self.api).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Get closed trading position history. Leave symbol empty for all symbols")]
    async fn get_trading_position_history(
        &self,
        Parameters(p): Parameters<OptionalSymbolParams>,
    ) -> String {
        let sym = if p.symbol.is_empty() { None } else { Some(p.symbol.as_str()) };
        match trading::get_trading_position_history(&self.api, sym).await {
            Ok(v)  => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }

    // ── Ticker ─────────────────────────────────────────────────────────────────

    #[tool(description = "Get 24-hour price statistics (OHLC, volume, change%) for a symbol")]
    async fn get_ticker(&self, Parameters(p): Parameters<SymbolParams>) -> String {
        match ticker::get_ticker(&self.api, &p.symbol).await {
            Ok(t)  => serde_json::to_string_pretty(&t).unwrap_or_else(|e| format!("error: {e}")),
            Err(e) => format!("error: {e}"),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FinanceMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_server_info(Implementation::new(
            "finance-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
    }
}
