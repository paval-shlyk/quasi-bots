use rmcp::{handler::server::wrapper::Json, tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

#[tool_router(router = knowledge_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "List all knowledge topics")]
    async fn knowledge_list_topics(&self) -> Result<Json<Vec<String>>, String> {
        let topics = knowledge::fetch_topics(&self.state.pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(Json(topics.into_iter().map(|t| t.name).collect()))
    }
}