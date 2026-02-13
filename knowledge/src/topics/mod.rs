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
    pub affinity_days: Option<u32>,
    /// Indicates whether the topic has been used in the current cycle. Once all topics have been
    /// used, this flag will be reset for all topics.
    pub is_used: bool,
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
            WHERE disabled_until IS NULL OR disabled_until <= CURRENT_TIMESTAMP
        "#
    )
    .fetch_one(pool)
    .await?
    .count;

    if count == 0 {
        return Err(anyhow::anyhow!("No topics are available"));
    }

    async fn fetch_and_update_topic(
        pool: &sqlx::SqlitePool,
    ) -> anyhow::Result<Option<Topic>> {
        let maybe_topic = sqlx::query!(
            r#"
                SELECT id, name
                FROM topic
                WHERE is_used = FALSE AND (disabled_until IS NULL OR disabled_until <= CURRENT_TIMESTAMP)
                ORDER BY RANDOM()
                LIMIT 1
            "#
        )
        .fetch_optional(pool)
        .await?;

        let Some(topic) = maybe_topic else {
            return Ok(None);
        };
        tracing::info!("Updating topic with id {} as used", topic.id);

        // Mark the topic as used
        // This action trigger `trg_mark_topic_disabled` which
        // will set `disabled_until` to `CURRENT_TIMESTAMP + INTERVAL 'N day'` where
        // N is the affinity days. If affinity days is set to NULL, the topic will not be disabled.
        // But still marked as used, so it won't be selected again until all other topics are used.
        sqlx::query!(
            r#"
                UPDATE topic
                SET is_used = TRUE
                WHERE id = ?
            "#,
            topic.id
        )
        .execute(pool)
        .await?;

        Ok(Some(Topic {
            id: topic.id as u64,
            name: topic.name,
        }))
    }

    let maybe_topic = fetch_and_update_topic(pool).await?;

    match maybe_topic {
        Some(topic) => Ok(topic),
        None => {
            tracing::info!(
                "All topics have been used, resetting is_used flags"
            );

            sqlx::query!(
                r#"
                    UPDATE topic
                    SET is_used = FALSE
                "#
            )
            .execute(pool)
            .await?;

            let topic =
                fetch_and_update_topic(pool).await?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Failed to fetch a topic after resetting is_used flags"
                    )
                })?;

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
                disabled_until as "disabled_until: chrono::DateTime<chrono::Utc>",
                affinity_days as "affinity_days: u32",
                is_used
            FROM (
                SELECT
                    t.id as id,
                    t.name,
                    COUNT(e.id) as questions_count,
                    t.disabled_until,
                    t.affinity_days,
                    t.is_used
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
