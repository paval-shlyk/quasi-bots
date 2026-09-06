use finance::{
    AnalysisServices, PositionInclude, PositionQuery,
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
    /// Extra blocks to attach. Allowed: trades, indicators, earnings, targets, news.
    /// Default is lean holdings only.
    #[serde(default)]
    include: Vec<PositionInclude>,
    /// Optional symbol filter; omit = all open names.
    #[serde(default)]
    symbols: Option<Vec<String>>,
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

    #[tool(description = "Fetch opened trading positions")]
    async fn trading_positions(
        &self,
        Parameters(args): Parameters<TradingPositionsArgs>,
    ) -> Result<Json<finance::OwningAssets>, String> {
        let query = PositionQuery {
            include: args.include,
            symbols: args.symbols,
        };
        let want_news = query.wants(PositionInclude::News);
        let want_targets = query.wants(PositionInclude::Targets);
        let want_earnings = query.wants(PositionInclude::Earnings);
        let want_indicators = query.wants(PositionInclude::Indicators);

        if !want_news && !want_targets && !want_earnings && !want_indicators {
            return finance::fetch_owning_assets(
                self.state.finance_state.api(),
                &query,
            )
            .await
            .map(Json)
            .map_err(|e| e.to_string());
        }

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

        finance::fetch_owning_assets_with_analysis(
            self.state.finance_state.api(),
            &services,
            &query,
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }
}
