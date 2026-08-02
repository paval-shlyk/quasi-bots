use rmcp::{handler::server::wrapper::Json, tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

#[tool_router(router = trading_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "Fetch trading portfolio summary")]
    async fn finance_portfolio(
        &self,
    ) -> Result<Json<finance::Portfolio>, String> {
        finance::portfolio::fetch_portfolio(self.state.finance_state.api())
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }
}
