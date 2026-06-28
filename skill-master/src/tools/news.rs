use rmcp::{tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

#[tool_router(router = news_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "News domain placeholder — full tools coming in a later PR")]
    async fn news_status(&self) -> String {
        format!(
            "news ready ({} configured sources)",
            self.state.config.news.rss_sources.len()
        )
    }
}