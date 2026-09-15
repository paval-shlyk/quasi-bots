use chrono::{DateTime, Utc};
use finance::{
    AnalysisInclude, AnalysisServices, UpsertWatch, WatchChannel, WatchCompare,
    WatchRule,
    analysis::{
        AssetNewsItem, FinnhubProvider, NewsProvider, YahooPriceTargetProvider,
    },
    indicators::AnalysisConfig,
};
use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::mcp::server::SkillMasterMcpServer;

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct TradingPositionsArgs {
    /// Optional symbol filter; omit = all open names.
    #[serde(default)]
    symbols: Option<Vec<String>>,
    /// When true, include per-lot `trades`. Default false = lean book.
    /// Research digs use `trading_analysis`, not this tool.
    #[serde(default)]
    include_trades: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TradingAnalysisArgs {
    /// Symbols to research. Required; at least one.
    symbols: Vec<String>,
    /// Blocks to attach. Allowed: news, targets, earnings, indicators.
    /// Empty / omitted means all four.
    #[serde(default)]
    include: Vec<AnalysisInclude>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TradingQuotesArgs {
    /// Broker / book symbols to mark (e.g. TSLA, Gold, US500). Required; 1..=30.
    symbols: Vec<String>,
    /// Reserved for custom windows; ignored in A1 (always skipped).
    #[serde(default)]
    include_klines: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TradingWatchesUpsertArgs {
    /// Existing watch id to update; omit to create.
    #[serde(default)]
    id: Option<i64>,
    /// Required for symbol rules; must be null for portfolio rules.
    #[serde(default)]
    symbol: Option<String>,
    /// day_change_pct | mark_vs_entry_pct | nav_day_change_pct | cash_below | weight_book_pct_above
    rule: WatchRule,
    threshold: f64,
    /// lte | gte | abs_gte
    compare: WatchCompare,
    /// telegram | mcp | both — delivery target; no tokens exposed here.
    channel: WatchChannel,
    /// Default 3600 when omitted.
    #[serde(default)]
    cooldown_secs: Option<i64>,
    /// Default true when omitted on create.
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TradingWatchIdArgs {
    id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TradingWatchesEnableArgs {
    id: i64,
    enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TradingAlertsListArgs {
    /// RFC3339 lower bound on `created_at`; omit = no lower bound.
    #[serde(default)]
    since: Option<String>,
    /// Max rows (1..=200). Default 50.
    #[serde(default)]
    limit: Option<i64>,
    /// When true, only unacked rows with channel mcp|both.
    #[serde(default)]
    pending_mcp_only: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TradingAlertsAckArgs {
    /// Outbox `event_id` values to mark seen by the agent.
    event_ids: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(crate = "serde")]
struct DeleteWatchResult {
    deleted: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(crate = "serde")]
struct AckAlertsResult {
    acked: u64,
}

use serde::Serialize;

struct NewsBankProvider {
    pool: sqlx::SqlitePool,
    limit: usize,
}

impl NewsProvider for NewsBankProvider {
    async fn recent(
        &self,
        symbol: &str,
        name: Option<&str>,
    ) -> anyhow::Result<Vec<AssetNewsItem>> {
        let mut needles = vec![symbol.to_string()];
        if let Some(n) = name
            && !n.is_empty()
        {
            needles.push(n.to_string());
        }
        let rows = news::select_recent_for_instrument(
            &self.pool,
            &needles,
            self.limit as i64,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|h| AssetNewsItem {
                title: h.title,
                url: h.url,
                published_at: Some(h.published_at),
                summary: None,
                source: h.source,
            })
            .collect())
    }
}

fn parse_since(raw: Option<String>) -> Result<Option<DateTime<Utc>>, String> {
    match raw {
        None => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(&s)
            .map(|dt| Some(dt.with_timezone(&Utc)))
            .map_err(|e| format!("invalid since RFC3339: {e}")),
    }
}

#[tool_router(router = investment_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(
        description = "Fetch trading portfolio wallet snapshot. cash = USD free; reserved_cash = USD locked (broker-reserved). nav = cash + reserved_cash (wallet Equity) — never cash + positions_value. positions_value is Σ open marks (CFD notionals may dwarf nav). margin_used = Σ Dzengi TradingPosition.margin for all open long lots (scaled by remaining qty), aggregated across every leveraged product — NOT total locked margin and often << reserved_cash; use reserved_cash for locked cash. buying_power = cash. realized_pnl is always null (P1, not wired). Legacy current_volume was dropped; use cash/reserved_cash/nav/historical_volume."
    )]
    async fn trading_portfolio(
        &self,
    ) -> Result<Json<finance::Portfolio>, String> {
        finance::investment::fetch_portfolio(self.state.finance_state.api())
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Fetch opened trading positions (Dzengi book: size, mark, P/L). Lean by default (no lots); pass include_trades=true for lots. Digs use trading_analysis. Each asset: leverage=true for CFD/leveraged names (exchangeInfo or *_LEVERAGE). Per-lot broker margin is not on these rows — portfolio margin_used (trading_portfolio) sums TradingPosition.margin across open longs; reserved_cash is wallet locked cash (often larger). Weights: weight_book_pct = |MV|/positions_value×100 (Σ≈100%), weight_nav_pct = |MV|/NAV×100 (CFD may Σ>100%); weight_percentage is a deprecated compat alias (NAV when known, else book)."
    )]
    async fn trading_positions(
        &self,
        Parameters(args): Parameters<TradingPositionsArgs>,
    ) -> Result<Json<finance::OwningAssets>, String> {
        finance::fetch_owning_assets(
            self.state.finance_state.api(),
            args.symbols.as_deref(),
            args.include_trades,
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }

    #[tool(
        description = "News, analyst targets, earnings, and technicals for named symbols. Pass include e.g. [\"indicators\"]. Mapping/history failures return per-symbol error (no silent wrong-instrument TA)."
    )]
    async fn trading_analysis(
        &self,
        Parameters(args): Parameters<TradingAnalysisArgs>,
    ) -> Result<Json<finance::AssetAnalysis>, String> {
        if args.symbols.is_empty() {
            return Err("trading_analysis requires at least one symbol".into());
        }

        let include = if args.include.is_empty() {
            AnalysisInclude::all()
        } else {
            args.include
        };
        let want_news = include.contains(&AnalysisInclude::News);
        let want_targets = include.contains(&AnalysisInclude::Targets);
        let want_earnings = include.contains(&AnalysisInclude::Earnings);
        let want_indicators = include.contains(&AnalysisInclude::Indicators);

        let services = AnalysisServices {
            news: want_news.then(|| NewsBankProvider {
                pool: self.state.news_state.pool.clone(),
                limit: 3,
            }),
            targets: want_targets.then(YahooPriceTargetProvider::new),
            earnings: want_earnings.then(|| {
                FinnhubProvider::new(
                    &self.state.finance_state.config.finn_hub_api_key,
                )
            }),
            technicals: want_indicators,
            technicals_config: AnalysisConfig::default(),
        };

        finance::fetch_asset_analysis(
            self.state.finance_state.api(),
            &services,
            &args.symbols,
            &include,
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Fetch live marks and day moves for named symbols (Dzengi WS primary, REST ticker/24hr fallback). Soft per-symbol errors; no Yahoo, no news/RSI, no Telegram. Cap 30 symbols. include_klines is accepted but unused in A1."
    )]
    async fn trading_quotes(
        &self,
        Parameters(args): Parameters<TradingQuotesArgs>,
    ) -> Result<Json<finance::QuotesResponse>, String> {
        let _ = args.include_klines; // A1: skip klines unless trivial later
        finance::fetch_quotes(self.state.finance_state.api(), &args.symbols)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "List durable trading watch rules (enabled and disabled). No Telegram tokens."
    )]
    async fn trading_watches_list(
        &self,
    ) -> Result<Json<finance::WatchList>, String> {
        finance::list_watches(self.state.finance_state.pool())
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Create or update one trading watch. Symbol required for day_change_pct / mark_vs_entry_pct / weight_book_pct_above; must be null for nav_day_change_pct / cash_below. Channel selects telegram|mcp|both — tokens stay in Vault, never in MCP."
    )]
    async fn trading_watches_upsert(
        &self,
        Parameters(args): Parameters<TradingWatchesUpsertArgs>,
    ) -> Result<Json<finance::Watch>, String> {
        finance::upsert_watch(
            self.state.finance_state.pool(),
            UpsertWatch {
                id: args.id,
                symbol: args.symbol,
                rule: args.rule,
                threshold: args.threshold,
                compare: args.compare,
                channel: args.channel,
                cooldown_secs: args.cooldown_secs,
                enabled: args.enabled,
            },
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }

    #[tool(description = "Delete a trading watch by id.")]
    async fn trading_watches_delete(
        &self,
        Parameters(args): Parameters<TradingWatchIdArgs>,
    ) -> Result<Json<DeleteWatchResult>, String> {
        let deleted =
            finance::delete_watch(self.state.finance_state.pool(), args.id)
                .await
                .map_err(|e| e.to_string())?;
        Ok(Json(DeleteWatchResult { deleted }))
    }

    #[tool(
        description = "Enable or disable a trading watch. Disabled watches never fire."
    )]
    async fn trading_watches_enable(
        &self,
        Parameters(args): Parameters<TradingWatchesEnableArgs>,
    ) -> Result<Json<finance::Watch>, String> {
        finance::set_watch_enabled(
            self.state.finance_state.pool(),
            args.id,
            args.enabled,
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }

    #[tool(
        description = "List recent alert outbox events (watch.fired). Optional since (RFC3339), limit, pending_mcp_only. No Telegram tokens."
    )]
    async fn trading_alerts_list(
        &self,
        Parameters(args): Parameters<TradingAlertsListArgs>,
    ) -> Result<Json<finance::AlertList>, String> {
        let since = parse_since(args.since)?;
        finance::list_alerts(
            self.state.finance_state.pool(),
            since,
            args.limit,
            args.pending_mcp_only,
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Ack alert outbox events by event_id (marks acked_mcp_at). Telegram delivery acks separately."
    )]
    async fn trading_alerts_ack(
        &self,
        Parameters(args): Parameters<TradingAlertsAckArgs>,
    ) -> Result<Json<AckAlertsResult>, String> {
        let acked = finance::ack_alerts(
            self.state.finance_state.pool(),
            &args.event_ids,
        )
        .await
        .map_err(|e| e.to_string())?;
        Ok(Json(AckAlertsResult { acked }))
    }
}
