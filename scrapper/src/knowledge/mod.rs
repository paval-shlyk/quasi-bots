mod model;
mod routes;
mod topic_sequence;

pub use model::*;
pub use routes::*;
use topic_sequence::TopicSequence;

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
}

#[derive(Debug)]
pub struct Database {
    topics: TopicSequence,
    pool: sqlx::SqlitePool,
}

impl Database {
    //by default, database is only connected to sqlite file
    pub async fn connect(pool: sqlx::SqlitePool) -> anyhow::Result<Self> {
        //todo: drop tables somehow

        let topics: Vec<Topic> = sqlx::query_as!(
            Topic,
            r#"
                SELECT id as "id: u64", name
                FROM topic
            "#
        )
        .fetch_all(&pool)
        .await?;

        Ok(Self {
            topics: TopicSequence::new(&topics),
            pool,
        })
    }

    pub async fn refresh_from_file(
        &mut self,
        file: &std::path::Path,
    ) -> anyhow::Result<()> {
        let raw_entries = tokio::fs::read_to_string(file)
            .await
            .expect("Failed to load knowledge file");

        let entries: Vec<Entry> = serde_yaml::from_str(&raw_entries)
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

        let topics: Vec<Topic> = sqlx::query_as!(
            Topic,
            r#"
                SELECT id as "id: u64", name
                FROM topic
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        self.topics = TopicSequence::new(&topics);

        Ok(())
    }

    /// Fetch new knowledge and mutate internal state
    pub async fn next_knowledge(&mut self) -> anyhow::Result<Entry> {
        if self.topics.is_empty() {
            return Err(anyhow::anyhow!("No topics available"));
        }

        let topic = {
            match self.topics.next() {
                Some(topic) => topic,
                None => {
                    self.topics.reset();

                    self.topics.next().expect("Sequence is reset")
                }
            }
        };

        let topic_id = topic.id as i64;

        #[derive(Debug, Clone, sqlx::FromRow)]
        struct EntryWithoutTags {
            pub id: i64,
            pub name: String,
            pub topic: String,
            pub question: String,
            pub truth: String,
        }
        async fn fetch_entry(
            pool: &sqlx::SqlitePool,
            topic_id: i64,
        ) -> anyhow::Result<Option<EntryWithoutTags>> {
            let maybe_entry = sqlx::query_as!(
                EntryWithoutTags,
                r#"
                SELECT e.id, e.name, t.name as topic, e.question, e.truth
                FROM entry as e
                JOIN topic as t ON e.topic_id = t.id
                WHERE t.id = ? AND e.reviewed_at IS NULL
                ORDER BY RANDOM()
                LIMIT 1
            "#,
                topic_id
            )
            .fetch_optional(pool)
            .await?;

            Ok(maybe_entry)
        }

        let entry = match fetch_entry(&self.pool, topic_id).await? {
            Some(entry) => entry,
            None => {
                // No unrevised entry, reset review status and try again
                sqlx::query!(
                    r#"
                        UPDATE entry
                        SET reviewed_at = NULL
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

        Ok(Entry {
            id: entry.name,
            topic: entry.topic,
            tags,
            question: entry.question,
            truth: entry.truth,
        })
    }

    pub async fn fetch_topics(
        &self,
    ) -> anyhow::Result<Vec<TopicWithStatistics>> {
        let topics: Vec<TopicWithStatistics> = sqlx::query_as!(
            TopicWithStatistics,
            r#"
            SELECT id as "id: u64", name, questions_count as "questions_count: u64"
            FROM (
                SELECT t.id as id, t.name, COUNT(e.id) as questions_count
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
}
