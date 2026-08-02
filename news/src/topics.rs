#[derive(Debug, serde::Serialize, utoipa::ToSchema, schemars::JsonSchema)]
pub struct NewsTopicList {
    pub topics: Vec<String>,
}

impl From<Vec<String>> for NewsTopicList {
    fn from(topics: Vec<String>) -> Self {
        Self { topics }
    }
}

pub async fn select_news_topics(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<String>> {
    let topics: Vec<_> = sqlx::query!("SELECT name FROM news_topic")
        .fetch_all(pool)
        .await?;

    let topics = topics.into_iter().map(|row| row.name).collect::<Vec<_>>();

    Ok(topics)
}
