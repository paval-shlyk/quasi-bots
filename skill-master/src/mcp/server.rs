use std::borrow::Cow;

use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool_handler,
};

use crate::AppState;

use super::schema::rewrite_tool_router_schemas;

const MCP_SERVER_NAME: &str = "skill-master-mcp";
const MCP_SERVER_TITLE: &str = "Skill Master";

/// Initialize / legacy-session fallback revision.
///
/// rmcp 3.2 still treats `LATEST` as `2025-11-25`. Modern clients may negotiate
/// `2026-07-28` via [`ServerHandler::supported_protocol_versions`]; those
/// requests are served statelessly by Streamable HTTP even while legacy
/// session mode remains enabled for older peers.
const MCP_PROTOCOL_FALLBACK: ProtocolVersion = ProtocolVersion::LATEST;

#[derive(Clone)]
pub struct SkillMasterMcpServer {
    pub(crate) state: AppState,
    tool_router: ToolRouter<Self>,
}

impl SkillMasterMcpServer {
    pub fn new(state: AppState) -> Self {
        let mut tool_router = Self::knowledge_tool_router()
            + Self::quotes_tool_router()
            + Self::search_tool_router()
            + Self::news_tool_router()
            + Self::expenses_tool_router()
            + Self::investment_tool_router();

        // schemars/rmcp emit Option<T> as `"type": ["T","null"]`. Rewrite once
        // for all tools so strict MCP clients that expect a single string
        // `type` still see nullability via anyOf.
        rewrite_tool_router_schemas(&mut tool_router);

        Self { state, tool_router }
    }

    /// Build the full tool router with MCP-compatible nullable schemas.
    ///
    /// Used by tests without constructing a full [`AppState`].
    pub fn tool_router_with_mcp_schemas() -> ToolRouter<Self> {
        let mut tool_router = Self::knowledge_tool_router()
            + Self::quotes_tool_router()
            + Self::search_tool_router()
            + Self::news_tool_router()
            + Self::expenses_tool_router()
            + Self::investment_tool_router();
        rewrite_tool_router_schemas(&mut tool_router);
        tool_router
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
        .with_server_info(
            Implementation::new(
                MCP_SERVER_NAME,
                crate::version::mcp_server_version(),
            )
            .with_title(MCP_SERVER_TITLE),
        )
        .with_protocol_version(MCP_PROTOCOL_FALLBACK)
        .with_instructions(
            "Skill-master MCP server. Call knowledge, quotes, news, and finance libraries directly.",
        )
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        // Advertise every revision rmcp knows, including modern `2026-07-28`.
        // Narrow later (phase 2/3) once live clients no longer need older
        // initialize/session peers.
        Cow::Borrowed(ProtocolVersion::KNOWN_VERSIONS)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::mcp::schema::contains_nullable_type_array;

    #[test]
    fn tool_input_schemas_use_anyof_not_nullable_type_arrays() {
        let router = SkillMasterMcpServer::tool_router_with_mcp_schemas();
        let tools = router.list_all();
        assert!(!tools.is_empty(), "expected registered MCP tools");

        for tool in &tools {
            let input = Value::Object((*tool.input_schema).clone());
            assert!(
                !contains_nullable_type_array(&input),
                "tool `{}` inputSchema still has nullable type arrays: {input}",
                tool.name
            );

            if let Some(output) = &tool.output_schema {
                let output = Value::Object((**output).clone());
                assert!(
                    !contains_nullable_type_array(&output),
                    "tool `{}` outputSchema still has nullable type arrays: {output}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn known_nullable_fields_emit_anyof() {
        let router = SkillMasterMcpServer::tool_router_with_mcp_schemas();
        let tools = router.list_all();

        let positions = tools
            .iter()
            .find(|t| t.name == "trading_positions")
            .expect("trading_positions tool");
        let symbols = positions
            .input_schema
            .get("properties")
            .and_then(|p| p.get("symbols"))
            .expect("symbols property");
        assert!(
            symbols.get("anyOf").is_some(),
            "trading_positions.symbols should be anyOf; got {symbols}"
        );
        assert!(
            symbols.get("type").is_none(),
            "trading_positions.symbols must not use a type array"
        );

        let list_entries = tools
            .iter()
            .find(|t| t.name == "expenses_list_entries")
            .expect("expenses_list_entries tool");
        let year = list_entries
            .input_schema
            .get("properties")
            .and_then(|p| p.get("year"))
            .expect("year property");
        assert!(
            year.get("anyOf").is_some(),
            "expenses_list_entries.year should be anyOf; got {year}"
        );
    }

    #[test]
    fn advertises_modern_and_legacy_protocol_versions() {
        let versions = ProtocolVersion::KNOWN_VERSIONS;
        assert!(
            versions.contains(&ProtocolVersion::V_2025_11_25),
            "legacy initialize fallback must remain advertised"
        );
        assert!(
            versions.contains(&ProtocolVersion::V_2026_07_28),
            "modern stateless revision must be advertised"
        );
        assert_eq!(MCP_PROTOCOL_FALLBACK, ProtocolVersion::V_2025_11_25);
        assert_eq!(MCP_PROTOCOL_FALLBACK, ProtocolVersion::LATEST);
    }

    #[test]
    fn get_info_uses_legacy_fallback_and_tools_capability() {
        let info = ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(
            Implementation::new(MCP_SERVER_NAME, "0.0.0+test")
                .with_title(MCP_SERVER_TITLE),
        )
        .with_protocol_version(MCP_PROTOCOL_FALLBACK);

        assert_eq!(info.protocol_version, ProtocolVersion::V_2025_11_25);
        assert!(info.capabilities.tools.is_some());
        assert_eq!(info.server_info.name, MCP_SERVER_NAME);
        assert_eq!(info.server_info.title.as_deref(), Some(MCP_SERVER_TITLE));
    }
}
