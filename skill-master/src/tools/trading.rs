use rmcp::{handler::server::wrapper::Json, tool};

use crate::{mcp::server::SkillMasterMcpServer, tools::util::json};

impl SkillMasterMcpServer {
    #[tool(description = "Fetch trading portfolio summary")]
    async fn finance_portfolio(
        &self,
    ) -> Result<Json<serde_json::Value>, String> {
        finance::portfolio::fetch_portfolio(self.state.finance_state.api())
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }
}
