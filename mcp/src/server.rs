use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::router::{prompt::PromptRouter, tool::ToolRouter},
    model::{
        AnnotateAble, CompleteRequestParams, CompleteResult, CompletionInfo,
        GetPromptRequestParams, GetPromptResult, Implementation,
        ListPromptsResult, ListResourcesResult,
        LoggingMessageNotificationParam, PaginatedRequestParams, PromptMessage,
        PromptMessageRole, ProtocolVersion, RawResource,
        ReadResourceRequestParams, ReadResourceResult, Reference,
        ResourceContents, ServerCapabilities, ServerInfo,
        SetLevelRequestParams,
    },
    prompt, prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router,
};

const SERVER_INFO_URI: &str = "mcp://hello-world/server-info";

#[derive(Clone)]
pub struct HelloWorldMcpServer {
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

#[tool_router]
impl HelloWorldMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    #[tool(description = "Get random value")]
    async fn generate_random_value(&self) -> String {
        42.to_string()
    }
}

#[prompt_router]
impl HelloWorldMcpServer {
    #[prompt(description = "A hello-world prompt template")]
    async fn hello(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            "Say hello from the MCP server.".to_string(),
        )]
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for HelloWorldMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .enable_prompts()
                .enable_prompts_list_changed()
                .enable_resources()
                .enable_resources_list_changed()
                .enable_logging()
                .enable_completions()
                .build(),
        )
        .with_server_info(Implementation::new(
            "hello-world-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
        .with_instructions(
            "Standalone hello-world MCP server for protocol compatibility testing.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                RawResource::new(SERVER_INFO_URI, "server-info")
                    .with_title("Server Info")
                    .with_description("Static hello-world server metadata")
                    .with_mime_type("application/json")
                    .no_annotation(),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if request.uri != SERVER_INFO_URI {
            return Err(McpError::resource_not_found(
                format!("Unknown resource: {}", request.uri),
                Some(serde_json::json!({ "uri": request.uri })),
            ));
        }

        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(
                SERVER_INFO_URI,
                r#"{"name":"hello-world-mcp","status":"ok"}"#,
            )
            .with_mime_type("application/json"),
        ]))
    }

    async fn set_level(
        &self,
        request: SetLevelRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        context
            .peer
            .notify_logging_message(LoggingMessageNotificationParam::new(
                request.level,
                serde_json::json!({
                    "message": format!("Log level set to {:?}", request.level),
                    "server": "hello-world-mcp",
                }),
            ))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(())
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let suggestions = match &request.r#ref {
            Reference::Prompt(prompt)
                if prompt.name == "hello"
                    && request.argument.name == "tone" =>
            {
                vec!["friendly".into(), "formal".into(), "playful".into()]
            }
            _ => Vec::new(),
        };

        let values: Vec<String> = suggestions
            .into_iter()
            .filter(|s: &String| s.starts_with(&request.argument.value))
            .collect();

        let completion = CompletionInfo::new(values)
            .map_err(|e| McpError::internal_error(e, None))?;

        Ok(CompleteResult::new(completion))
    }
}

