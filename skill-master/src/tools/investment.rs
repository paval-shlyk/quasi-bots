use finance::{
    AnalysisServices,
    analysis::{FinnhubProvider, RssNewsProvider, YahooPriceTargetProvider},
    indicators::AnalysisConfig,
};
use rmcp::{handler::server::wrapper::Json, tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

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
    ) -> Result<Json<finance::OwningAssets>, String> {
        let services = AnalysisServices {
            news: RssNewsProvider::with_limit(5).into(),
            targets: YahooPriceTargetProvider::new().into(),
            earnings: FinnhubProvider::new(
                &self.state.finance_state.config.finn_hub_api_key,
            )
            .into(),
            technicals: true,
            technicals_config: AnalysisConfig::default(),
        };

        finance::fetch_owning_assets_with_analysis(
            self.state.finance_state.api(),
            &services,
        )
        .await
        .map(Json)
        .map_err(|e| e.to_string())
    }
}
