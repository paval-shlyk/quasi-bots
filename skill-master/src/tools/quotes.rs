use rmcp::{Json, tool, tool_router};

use crate::mcp::server::SkillMasterMcpServer;

#[tool_router(router = quotes_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "List known quote authors with quote counts")]
    async fn quotes_list_authors(
        &self,
    ) -> Result<Json<crate::quotes::QuoteAuthorList>, String> {
        crate::quotes::fetch_known_authors(&self.state.pool)
            .await
            .map(crate::quotes::QuoteAuthorList::from)
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Fetch the next unused famous quote")]
    async fn quotes_next(
        &self,
    ) -> Result<Json<crate::quotes::FamousQuote>, String> {
        match crate::quotes::fetch_next_unused_quote(&self.state.pool).await {
            Ok(Some(quote)) => Ok(Json(quote)),
            Ok(None) => {
                crate::quotes::notify_needs_more_quotes(&self.state);
                Err("No fresh quotes available, retry in ~15s".into())
            }
            Err(e) => Err(e.to_string()),
        }
    }
}
