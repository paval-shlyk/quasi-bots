pub async fn select_news_topics(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<String>> {
    let topics: Vec<_> = sqlx::query!("SELECT name FROM news_topic")
        .fetch_all(pool)
        .await?;

    let topics = topics.into_iter().map(|row| row.name).collect::<Vec<_>>();

    Ok(topics)
}
