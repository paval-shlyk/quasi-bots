use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool_handler,
};

use crate::AppState;

use super::schema::rewrite_tool_router_schemas;

const MCP_SERVER_NAME: &str = "skill-master-mcp";

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
        .with_server_info(Implementation::new(
            MCP_SERVER_NAME,
            crate::version::mcp_server_version(),
        ))
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
        .with_instructions(
            "Skill-master MCP server. Call knowledge, quotes, news, and finance libraries directly.",
        )
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
    fn trading_quotes_tool_registered_with_anyof_nullables() {
        let router = SkillMasterMcpServer::tool_router_with_mcp_schemas();
        let tools = router.list_all();
        let quotes = tools
            .iter()
            .find(|t| t.name == "trading_quotes")
            .expect("trading_quotes tool");

        assert!(
            quotes
                .input_schema
                .get("properties")
                .and_then(|p| p.get("symbols"))
                .is_some(),
            "trading_quotes input must declare symbols"
        );

        let output = quotes
            .output_schema
            .as_ref()
            .expect("trading_quotes outputSchema");
        let output = Value::Object((**output).clone());
        assert!(
            !contains_nullable_type_array(&output),
            "trading_quotes outputSchema has nullable type arrays: {output}"
        );

        let last = output
            .pointer("/properties/quotes/items/properties/last")
            .or_else(|| {
                output
                    .pointer("/properties/quotes/items/anyOf/0/properties/last")
            })
            .or_else(|| output.pointer("/$defs/Quote/properties/last"))
            .or_else(|| output.pointer("/definitions/Quote/properties/last"))
            .or_else(|| {
                let pref = output
                    .pointer("/properties/quotes/items/$ref")
                    .and_then(|r| r.as_str())?;
                let name = pref.rsplit('/').next()?;
                output
                    .pointer(&format!("/$defs/{name}/properties/last"))
                    .or_else(|| {
                        output.pointer(&format!(
                            "/definitions/{name}/properties/last"
                        ))
                    })
            })
            .unwrap_or_else(|| {
                panic!("could not locate quotes[].last in schema: {output}")
            });
        assert!(
            last.get("anyOf").is_some(),
            "quotes[].last should be anyOf; got {last}"
        );
    }
}
