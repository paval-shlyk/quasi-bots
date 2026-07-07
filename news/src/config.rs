use std::time::Duration;

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
    #[serde(alias = "rss_source", default)]
    pub rss_sources: Vec<RssSource>,
    #[serde(default)]
    pub gemini_config: Option<crate::llm::GeminiConfig>,

    /// Timeout when new fetch session will be started
    #[serde(with = "humantime_serde")]
    pub refresh_timeout: Duration,

    /// Timeout when data will persist in storage
    #[serde(with = "humantime_serde")]
    pub article_max_age: Duration,

    /// Number of retry attempts for fetching a source before marking it as broken
    pub retry_attempts: u32,
    #[serde(with = "humantime_serde")]
    pub broken_link_cooldown: Duration,
}
