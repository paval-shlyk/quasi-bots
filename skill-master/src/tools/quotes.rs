use rmcp::{tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

use super::util::json;

#[tool_router(router = quotes_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "List known quote authors with quote counts")]
    async fn quotes_list_authors(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<serde_json::Value>, String> {
        crate::quotes::fetch_known_authors(&self.state.pool)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(description = "Fetch the next unused famous quote")]
    async fn quotes_next(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<serde_json::Value>, String> {
        match crate::quotes::fetch_next_unused_quote(&self.state.pool).await {
            Ok(Some(quote)) => json(quote),
            Ok(None) => {
                crate::quotes::notify_needs_more_quotes(&self.state);
                Err("No fresh quotes available, please try again later".into())
            }
            Err(e) => Err(e.to_string()),
        }
    }
}