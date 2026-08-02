use rmcp::{handler::server::wrapper::Json, tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

#[tool_router(router = news_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "Today's news briefing grouped by topic")]
    async fn today(&self) -> Result<Json<news::TodayNews>, String> {
        news::select_today_articles(&self.state.news_state.pool)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(description = "List broken RSS links and retry schedule")]
    async fn broken_links(&self) -> Result<Json<news::BrokenLinks>, String> {
        news::fetch_broken_links(&self.state.news_state)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(description = "RSS source statistics")]
    async fn source_statistics(
        &self,
    ) -> Result<Json<news::SourceStatisticsList>, String> {
        news::select_source_with_statistics(&self.state.news_state.pool)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(description = "List configured news topic names")]
    async fn topics(&self) -> Result<Json<news::NewsTopicList>, String> {
        news::select_news_topics(&self.state.news_state.pool)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }
}
