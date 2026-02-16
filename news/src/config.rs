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
pub struct Config {
    #[serde(alias = "rss_source")]
    pub rss_sources: Vec<RssSource>,
    pub gemini_config: crate::llm::GeminiConfig,
}
