use std::path::PathBuf;

/// Morning briefing news sources
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RssSource {
    /// unique name for a group of news sources, e.g. "Tech News"
    pub topic: String,
    pub urls: Vec<reqwest::Url>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeDatabaseConfig {
    pub directory: PathBuf,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    #[serde(alias = "rss_source")]
    pub rss_sources: Vec<RssSource>,

    /// The news sources to use for the investment periodical news briefing
    #[serde(default)]
    pub investment_rss_sources: Vec<reqwest::Url>,

    /// SerpAPI key for fetching news from Google News
    pub serp_api_key: String,
}
