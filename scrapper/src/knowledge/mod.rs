mod affinity;
mod model;
mod reviews;
mod routes;

pub use affinity::*;
pub use model::*;
pub use reviews::*;
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

#[derive(Debug)]
pub struct Database {
    pool: sqlx::SqlitePool,
}

async fn next_topic(pool: &sqlx::SqlitePool) -> anyhow::Result<Topic> {
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

#[derive(Debug, Clone)]
pub enum KnowledgeMode {
    WithTag { tag: String },
    Random,
}

impl Database {
    //by default, database is only connected to sqlite file
    pub async fn connect(pool: sqlx::SqlitePool) -> anyhow::Result<Self> {
        Ok(Self { pool })
    }

    pub async fn refresh_from_file(
        &self,
        file: &std::path::Path,
    ) -> anyhow::Result<()> {
        let raw_entries = tokio::fs::read_to_string(file)
            .await
            .expect("Failed to load knowledge file");

        let entries: Vec<HumanEntry> = serde_yaml::from_str(&raw_entries)
            .expect("Invalid YAML format for knowledge file");

        sqlx::query!(
            r#"
                DELETE FROM m2m_entry_tag;
                DELETE FROM entry;
                DELETE FROM tag;
                DELETE FROM topic;
            "#
        )
        .execute(&self.pool)
        .await?;

        for entry in entries.iter() {
            let topic_id: i64 = sqlx::query!(
                r#"
                        INSERT INTO topic (name)
                        VALUES (?)
                        ON CONFLICT(name) DO UPDATE SET name=excluded.name
                        RETURNING id
                    "#,
                entry.topic
            )
            .fetch_one(&self.pool)
            .await?
            .id;

            let entry_id = sqlx::query!(
                r#"
                        INSERT INTO entry (topic_id, name, question, truth)
                        VALUES (?, ?, ?, ?)
                        RETURNING id
                    "#,
                topic_id,
                entry.id,
                entry.question,
                entry.truth
            )
            .fetch_one(&self.pool)
            .await?
            .id;

            for tag in entry.tags.iter() {
                let tag_id: i64 = sqlx::query!(
                        r#"
                                INSERT INTO tag (name)
                                VALUES (?)
                                ON CONFLICT(name) DO UPDATE SET name=excluded.name
                                RETURNING id
                            "#,
                        tag
                    )
                    .fetch_one(&self.pool)
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
                .execute(&self.pool)
                .await?;
            }
        }

        Ok(())
    }

    /// Fetch new knowledge and mutate internal state
    pub async fn next_knowledge(&self) -> anyhow::Result<HumanEntry> {
        let topic = next_topic(&self.pool).await?;

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

        let entry = match fetch_entry(&self.pool, topic_id).await? {
            Some(entry) => entry,
            None => {
                tracing::info!(
                    "No unrevised entry found for topic_id {}, resetting review status and trying again",
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
                .execute(&self.pool)
                .await?;

                fetch_entry(&self.pool, topic_id).await?.ok_or_else(|| {
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
        .execute(&self.pool)
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
        .fetch_all(&self.pool)
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

    pub async fn fetch_topics(
        &self,
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
        .fetch_all(&self.pool)
        .await?;

        Ok(topics)
    }

    pub async fn fetch_tags(&self) -> anyhow::Result<Vec<String>> {
        let tags = sqlx::query!(
            r#"
                SELECT name 
                FROM tag
            "#
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.name)
        .collect::<Vec<_>>();

        Ok(tags)
    }
}
