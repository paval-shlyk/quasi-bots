mod routes;

use crate::topics;

pub use routes::*;

/// Human readable entry for a question
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HumanEntry {
    // Unique identifier for the entry, can be a UUID or any string
    // used only for potential compatibility with external systems, not used for internal logic
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
                        e.affinity_days as "affinity_days: u32"
                    FROM entry as e
                    JOIN topic as t ON e.topic_id = t.id
                    WHERE t.id = ? AND e.is_reviewed = FALSE AND (e.disabled_until IS NULL OR e.disabled_until <= CURRENT_TIMESTAMP)
                    ORDER BY RANDOM()
                    LIMIT 1
                "#,
            topic_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(entry)
    }

    let topic = topics::fetch_random_topic(pool).await?;

    let topic_id = topic.id as i64;

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
    //This update also invokes two triggers:
    //`trg_mark_entry_disabled` and `trg_create_recent_review`.
    //
    //`trg_mark_entry_disabled` will disable the
    //entry for a certain period of time based on the affinity days (if not affinity is set then
    //trigger does nothing).
    //`trg_crate_recent_review` create a new record in the `review`
    //table with the current timestamp and remove old one (only one review exist).
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

pub async fn add_new_entry(
    pool: &sqlx::SqlitePool,
    topic_id: u64,
    question: String,
    truth: String,
    tags: Vec<String>,
) -> anyhow::Result<()> {
    let name = format!("custom_{}", uuid::Uuid::new_v4().to_string());

    let topic_id = topic_id as i64;

    let mut tx = pool.begin().await?;

    let entry_id = sqlx::query!(
        r#"
            INSERT INTO entry (name, topic_id, question, truth)
            VALUES (?, ?, ?, ?)
            RETURNING id
        "#,
        name,
        topic_id,
        question,
        truth
    )
    .fetch_one(tx.as_mut())
    .await?
    .id;

    for tag in tags {
        let tag_id = sqlx::query!(
            r#"
                INSERT INTO tag (name)
                VALUES (?)
                ON CONFLICT(name) DO UPDATE SET name=excluded.name
                RETURNING id
            "#,
            tag
        )
        .fetch_one(tx.as_mut())
        .await?
        .id;

        sqlx::query!(
            r#"
                INSERT INTO m2m_entry_tag (entry_id, tag_id)
                VALUES (?, ?)
            "#,
            entry_id,
            tag_id
        )
        .execute(tx.as_mut())
        .await?;
    }

    tx.commit().await?;

    Ok(())
}
