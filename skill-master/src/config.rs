use std::path::PathBuf;

/// Morning briefing news sources
#[derive(
    Clone, Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
pub struct RssSource {
    /// unique name for a group of news sources, e.g. "Tech News"
    pub topic: String,
    #[schema(value_type = Vec<String>)]
    pub urls: Vec<reqwest::Url>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeConfig {
    /// Where yaml configuration files are loaded
    pub database_file: PathBuf,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// sqlite database file path, e.g. "scrapper.db"
    pub db_file: String,
    #[serde(alias = "rss_source")]
    pub rss_sources: Vec<RssSource>,

    pub knowledge: KnowledgeConfig,
    /// The news sources to use for the investment periodical news briefing
    pub finance: finance::Config,

    /// SerpAPI key for fetching news from Google News
    pub serp_api_key: String,
}
