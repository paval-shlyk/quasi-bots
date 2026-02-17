use std::collections::HashMap;

use crate::{ArticlesWithTopic, SavedArticle};

pub async fn select_today_articles(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<ArticlesWithTopic>> {
    use sqlx::types::Json;

    let articles = sqlx::query_as!(
        SavedArticle,
        r#"
            SELECT 
                t.name as topic,
                a.title,
                a.content, 
                authors as "authors: Json<Vec<String>>",
                links as "links: Json<Vec<String>>",
                published_at as "published_at: chrono::DateTime<chrono::Utc>"
            FROM article as a
            JOIN topic as t ON a.topic_id = t.id
            WHERE a.published_at > datetime('now', '-1 days')
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::new();

    for a in articles {
        map.entry(a.topic.clone())
            .or_insert_with(Vec::new)
            .push(a.into_feed());
    }

    let articles = map
        .into_iter()
        .map(|(topic, articles)| ArticlesWithTopic { topic, articles })
        .collect::<Vec<_>>();

    Ok(articles)
}
