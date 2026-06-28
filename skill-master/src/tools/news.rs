use rmcp::{tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

use super::util::json;

#[tool_router(router = news_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "Today's news briefing grouped by topic")]
    async fn news_today(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<serde_json::Value>, String> {
        news::select_today_articles(&self.state.news_state.pool)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(description = "List broken RSS links and retry schedule")]
    async fn news_broken_links(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<serde_json::Value>, String> {
        news::fetch_broken_links(&self.state.news_state)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(description = "RSS source statistics")]
    async fn news_source_statistics(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<serde_json::Value>, String> {
        news::select_source_with_statistics(&self.state.news_state.pool)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(description = "List configured news topic names")]
    async fn news_topics(&self) -> Result<rmcp::handler::server::wrapper::Json<serde_json::Value>, String> {
        news::select_news_topics(&self.state.news_state.pool)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }
}