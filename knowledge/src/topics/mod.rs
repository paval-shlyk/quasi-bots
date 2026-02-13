mod routes;

pub use routes::*;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Topic {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct TopicWithStatistics {
    pub id: u64,
    pub name: String,
    pub questions_count: u64,
    pub disabled_until: Option<chrono::DateTime<chrono::Utc>>,
}

/// Fetches a random topic that has not been used yet. If all topics have been used, it resets the
/// is_used flag for all topics and fetches again.
pub async fn fetch_random_topic(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Topic> {
    let count = sqlx::query!(
        r#"
            SELECT COUNT(id) as count
            FROM topic
        "#
    )
    .fetch_one(pool)
    .await?
    .count;

    if count == 0 {
        return Err(anyhow::anyhow!("No topics are available"));
    }

    let maybe_topic = sqlx::query_as!(
        Topic,
        r#"
            SELECT id as "id: u64", name
            FROM topic
            WHERE is_used = FALSE
            ORDER BY RANDOM()
            LIMIT 1
        "#
    )
    .fetch_optional(pool)
    .await?;

    match maybe_topic {
        Some(topic) => Ok(topic),
        None => {
            sqlx::query!(
                r#"
                    UPDATE topic
                    SET is_used = FALSE;
                "#
            )
            .execute(pool)
            .await?;

            let topic = sqlx::query_as!(
                Topic,
                r#"
                    SELECT id as "id: u64", name
                    FROM topic
                    WHERE is_used = FALSE
                    ORDER BY RANDOM()
                    LIMIT 1
                "#
            )
            .fetch_one(pool)
            .await?;

            Ok(topic)
        }
    }
}

/// Fetches all topics with their associated statistics, such as the number of questions and
/// disabled status.
pub async fn fetch_topics(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<TopicWithStatistics>> {
    let topics: Vec<TopicWithStatistics> = sqlx::query_as!(
            TopicWithStatistics,
            r#"
            SELECT 
                id as "id: u64",
                name,
                questions_count as "questions_count: u64",
                disabled_until as "disabled_until: chrono::DateTime<chrono::Utc>"
            FROM (
                SELECT t.id as id, t.name, COUNT(e.id) as questions_count, t.disabled_until
                FROM topic as t
                LEFT JOIN entry as e ON e.topic_id = t.id
                GROUP BY t.id
            )
            ORDER BY questions_count DESC
            "#
        )
        .fetch_all(pool)
        .await?;

    Ok(topics)
}
