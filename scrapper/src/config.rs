#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RssSource {
    /// unique name for a group of news sources, e.g. "Tech News"
    pub topic: String,
    pub urls: Vec<reqwest::Url>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// morning briefing news sources
    #[serde(alias = "rss_source")]
    pub rss_sources: Vec<RssSource>,
}
