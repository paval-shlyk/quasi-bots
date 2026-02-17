pub async fn select_news_topics(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<String>> {
    let topics = sqlx::query!("SELECT name FROM news_topic")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.name)
        .collect::<Vec<_>>();

    Ok(topics)
}
