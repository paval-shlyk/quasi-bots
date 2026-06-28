use rmcp::{tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

#[tool_router(router = quotes_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "Get MCP server name")]
    async fn get_name(&self) -> String {
        "skill-master-mcp".to_string()
    }
}