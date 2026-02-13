mod routes;

use crate::topics;

pub use routes::*;

/// Human readable entry for a question
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HumanEntry {
    // Unique identifier for the entry, can be a UUID or any string
    pub id: String,
    pub topic: String,
    pub tags: Vec<String>,

    pub question: String,

    pub truth: String,

    /// Number of days until the next review, if the user has an affinity for this topic/entry,
    /// otherwise None
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub affinity_days: Option<u32>,
}

/// Fetch new knowledge and mutate internal state
pub async fn fetch_random_entry(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<HumanEntry> {
    let topic = topics::fetch_random_topic(pool).await?;

    let topic_id = topic.id as i64;

    #[derive(Debug, Clone, sqlx::FromRow)]
    struct EntryWithoutTags {
        pub id: i64,
        pub name: String,
        pub topic: String,
        pub question: String,
        pub truth: String,
        pub affinity_days: Option<u32>,
    }

    async fn fetch_entry(
        pool: &sqlx::SqlitePool,
        topic_id: i64,
    ) -> anyhow::Result<Option<EntryWithoutTags>> {
        let entry = sqlx::query_as!(
            EntryWithoutTags,
            r#"
                    SELECT 
                        e.id,
                        e.name,
                        t.name as topic,
                        e.question,
                        e.truth,
                        e.affinity_days as "affinity_days: Option<u32>"
                    FROM entry as e
                    JOIN topic as t ON e.topic_id = t.id
                    WHERE t.id = ? AND e.is_reviewed = FALSE
                    ORDER BY RANDOM()
                    LIMIT 1
                "#,
            topic_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(entry)
    }

    let entry = match fetch_entry(pool, topic_id).await? {
        Some(entry) => entry,
        None => {
            tracing::info!(
                "All entries for topic_id {} are used, resetting review status and trying again",
                topic_id
            );
            // No unrevised entry, reset review status and try again
            sqlx::query!(
                r#"
                        UPDATE entry
                        SET is_reviewed = FALSE
                        WHERE topic_id = ?
                    "#,
                topic_id
            )
            .execute(pool)
            .await?;

            fetch_entry(pool, topic_id).await?.ok_or_else(|| {
                    anyhow::anyhow!("Entries exist but failed to fetch after resetting review status")
                })?
        }
    };

    //also invoke trigger to create recent review
    sqlx::query!(
        r#"
                UPDATE entry
                SET is_reviewed = TRUE
                WHERE id = ?
            "#,
        entry.id
    )
    .execute(pool)
    .await?;

    let tags = sqlx::query!(
        r#"
                SELECT tag.name
                FROM m2m_entry_tag as mt
                JOIN tag ON mt.tag_id = tag.id
                WHERE mt.entry_id = ?
            "#,
        entry.id
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|record| record.name)
    .collect::<Vec<String>>();

    Ok(HumanEntry {
        id: entry.name,
        topic: entry.topic,
        tags,
        question: entry.question,
        truth: entry.truth,
        affinity_days: entry.affinity_days,
    })
}
