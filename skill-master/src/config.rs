use std::path::PathBuf;

pub use mcp_auth::McpAuthConfig;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeConfig {
    /// Where yaml configuration files are loaded
    pub database_file: PathBuf,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// sqlite database file path, e.g. "scrapper.db"
    pub db_file: String,
    pub news: news::Config,

    pub knowledge: KnowledgeConfig,
    /// The news sources to use for the investment periodical news briefing
    pub finance: finance::Config,

    /// SerpAPI key for fetching news from Google News
    pub serp_api_key: String,

    pub mcp: McpAuthConfig,
}
