use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
};

use crate::{mcp::server::SkillMasterMcpServer, search};

use super::util::json;

#[tool_router(router = search_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    // This API is indented to be used only by Agents, that lacks of native WebSearch API
    #[tool(description = "Search the web via SerpAPI (Google)")]
    async fn search(
        &self,
        Parameters(query): Parameters<search::SearchQuery>,
    ) -> Result<Json<serde_json::Value>, String> {
        let client = search::client();
        let api_key = &self.state.config.serp_api_key;

        search::perform_search(&client, api_key, &query.query)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }
}
