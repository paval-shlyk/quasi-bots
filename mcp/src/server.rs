use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ServerInfo},
    tool, tool_handler, tool_router,
};

#[derive(Clone)]
pub struct HelloWorldMcpServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl HelloWorldMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Get MCP server name")]
    async fn get_name(&self) -> String {
        "Biba 12".to_string()
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for HelloWorldMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_server_info(Implementation::new(
            "hello-world-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
    }
}
