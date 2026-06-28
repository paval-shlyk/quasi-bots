use rmcp::{tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

#[tool_router(router = finance_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "Finance domain placeholder — full tools coming in a later PR")]
    async fn finance_status(&self) -> String {
        "finance ready".to_string()
    }
}