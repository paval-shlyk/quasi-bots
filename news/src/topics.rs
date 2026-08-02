#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct NewsTopicList {
    pub topics: Vec<String>,
}

pub async fn select_news_topics(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<NewsTopicList> {
    let topics: Vec<_> = sqlx::query!("SELECT name FROM news_topic")
        .fetch_all(pool)
        .await?;

    let topics = topics.into_iter().map(|row| row.name).collect();

    Ok(NewsTopicList { topics })
}
