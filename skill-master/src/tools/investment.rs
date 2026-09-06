use finance::{
    AnalysisInclude, AnalysisServices,
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
}
