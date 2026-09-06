use finance::{
    AnalysisInclude, AnalysisServices, PositionsInclude,
    analysis::{
        AssetNewsItem, FinnhubProvider, NewsProvider, YahooPriceTargetProvider,
    },
    analysis_includes_from_positions,
    indicators::AnalysisConfig,
    positions_want_trades,
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
    /// Opt-in extras. Allowed: `trades`, `indicators`, `earnings`, `targets`,
    /// `news`. Empty / omitted = lean book (no lots, no research).
    #[serde(default)]
    include: Vec<PositionsInclude>,
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
    #[tool(description = "Fetch trading portfolio summary")]
    async fn trading_portfolio(
        &self,
    ) -> Result<Json<finance::Portfolio>, String> {
        finance::investment::fetch_portfolio(self.state.finance_state.api())
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Fetch opened trading positions (Dzengi book: size, mark, P/L). Default is lean (no lots). Pass include=[\"trades\"] for lots; include indicators/earnings/targets/news for research (or use trading_analysis)."
    )]
    async fn trading_positions(
        &self,
        Parameters(args): Parameters<TradingPositionsArgs>,
    ) -> Result<Json<finance::OwningAssets>, String> {
        let include_trades = positions_want_trades(&args.include);
        let mut owning = finance::fetch_owning_assets(
            self.state.finance_state.api(),
            args.symbols.as_deref(),
            include_trades,
        )
        .await
        .map_err(|e| e.to_string())?;

        let research = analysis_includes_from_positions(&args.include);
        if !research.is_empty() {
            let symbols: Vec<String> =
                if let Some(filter) = args.symbols.as_ref() {
                    filter.clone()
                } else {
                    owning
                        .assets
                        .iter()
                        .map(|a| a.asset.symbol.clone())
                        .collect()
                };
            if !symbols.is_empty() {
                let want_news = research.contains(&AnalysisInclude::News);
                let want_targets = research.contains(&AnalysisInclude::Targets);
                let want_earnings =
                    research.contains(&AnalysisInclude::Earnings);
                let want_indicators =
                    research.contains(&AnalysisInclude::Indicators);

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

                let analysis = finance::fetch_asset_analysis(
                    self.state.finance_state.api(),
                    &services,
                    &symbols,
                    &research,
                )
                .await
                .map_err(|e| e.to_string())?;
                owning.analysis = analysis.symbols;
            }
        }

        Ok(Json(owning))
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
