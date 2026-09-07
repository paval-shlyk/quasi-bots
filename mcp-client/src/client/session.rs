use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientInfo,
    Implementation, ProtocolVersion, Tool,
};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{ClientLifecycleMode, ClientServiceExt, RoleClient};

use crate::config::ConnectOptions;
use crate::model::{CallOutcome, ServerStatus, ToolView};
use crate::{Error, Result};

/// Active MCP client session over Streamable HTTP.
pub struct McpSession {
    service: RunningService<RoleClient, ClientInfo>,
    server: ServerStatus,
    url: String,
}

impl McpSession {
    /// Connect to a Streamable HTTP MCP endpoint.
    ///
    /// Prefers modern `server/discover` + `2026-07-28`, falling back to legacy
    /// `initialize` on `2025-11-25` when the peer does not speak the modern
    /// revision (`ClientLifecycleMode::Auto`).
    ///
    /// Requires `opts.token` (Bearer access token without the `Bearer ` prefix).
    pub async fn connect(opts: ConnectOptions) -> Result<Self> {
        let token = opts
            .token
            .clone()
            .filter(|t| !t.is_empty())
            .ok_or(Error::AuthRequired)?;

        tracing::info!(
            url = %opts.url,
            token_len = token.len(),
            "connecting to MCP endpoint"
        );

        let mut config =
            StreamableHttpClientTransportConfig::with_uri(opts.url.as_str());
        config = config.auth_header(token);
        // skill-master keeps legacy_session_mode for older peers; modern
        // 2026-07-28 negotiation is always stateless — allow both.
        config.allow_stateless = true;

        let transport = StreamableHttpClientTransport::from_config(config);

        let client_info = ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("mcp-client", env!("CARGO_PKG_VERSION")),
        )
        // Hint for legacy initialize fallback path.
        .with_protocol_version(ProtocolVersion::V_2025_11_25);

        let lifecycle = ClientLifecycleMode::Auto {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            legacy_version: Some(ProtocolVersion::V_2025_11_25),
        };

        let service = client_info
            .serve_with_lifecycle(transport, lifecycle)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, url = %opts.url, "MCP discover/initialize failed");
                Error::service(e)
            })?;

        let server = service
            .peer_info()
            .map(|info| ServerStatus::from(info.as_ref()))
            .unwrap_or(ServerStatus {
                name: "unknown".into(),
                version: "?".into(),
                protocol_version: ProtocolVersion::V_2025_11_25.to_string(),
                instructions: None,
                tools_enabled: false,
            });

        tracing::info!(
            server = %server.name,
            version = %server.version,
            protocol = %server.protocol_version,
            "MCP session ready"
        );

        Ok(Self {
            service,
            server,
            url: opts.url,
        })
    }

    pub fn server_status(&self) -> &ServerStatus {
        &self.server
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// List all tools (follows pagination).
    pub async fn list_tools(&self) -> Result<Vec<ToolView>> {
        let tools = self
            .service
            .peer()
            .list_all_tools()
            .await
            .map_err(Error::service)?;
        Ok(tools.iter().map(ToolView::from).collect())
    }

    /// List raw `rmcp` tools (for advanced callers).
    pub async fn list_tools_raw(&self) -> Result<Vec<Tool>> {
        self.service
            .peer()
            .list_all_tools()
            .await
            .map_err(Error::service)
    }

    /// Call a tool with a JSON object/array/null arguments value.
    ///
    /// Non-object values are wrapped as empty object when `null`.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallOutcome> {
        let args_map = match arguments {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            other => {
                return Err(Error::InvalidArguments(format!(
                    "tool arguments must be a JSON object, got {other}"
                )));
            }
        };

        let params = CallToolRequestParams::new(name.to_string())
            .with_arguments(args_map.unwrap_or_default());

        let result: CallToolResult = self
            .service
            .peer()
            .call_tool(params)
            .await
            .map_err(Error::service)?;

        Ok(CallOutcome::from(&result))
    }

    /// Gracefully close the session.
    pub async fn disconnect(self) -> Result<()> {
        let _ = self.service.cancel().await;
        Ok(())
    }
}
