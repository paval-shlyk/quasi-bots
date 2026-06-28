use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool_handler,
};

use crate::AppState;

const MCP_SERVER_NAME: &str = "skill-master-mcp";

#[derive(Clone)]
pub struct SkillMasterMcpServer {
    pub(crate) state: AppState,
    tool_router: ToolRouter<Self>,
}

impl SkillMasterMcpServer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::knowledge_tool_router()
                + Self::quotes_tool_router()
                + Self::search_tool_router()
                + Self::news_tool_router()
                + Self::finance_tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SkillMasterMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(Implementation::new(
            MCP_SERVER_NAME,
            env!("CARGO_PKG_VERSION"),
        ))
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
        .with_instructions(
            "Skill-master MCP server. Call knowledge, quotes, news, and finance libraries directly.",
        )
    }
}

