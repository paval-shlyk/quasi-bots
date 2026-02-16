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
    #[serde(alias = "rss_source")]
    pub rss_sources: Vec<RssSource>,
    pub gemini_config: crate::llm::GeminiConfig,

    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub refresh_timeout: Duration,
}

fn serialize_duration<S>(
    duration: &Duration,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_u64(duration.as_secs() / 60)
}

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    let secs = u64::deserialize(deserializer)?;
    Ok(Duration::from_mins(secs))
}
