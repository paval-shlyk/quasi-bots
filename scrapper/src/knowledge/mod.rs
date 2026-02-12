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
            topics: TopicSequence::from_slice(&topics),
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
                DELETE FROM entry;
                DELETE FROM topic;
            "#
        )
        .execute(&self.pool)
        .await?;

        todo!()
        // let topics = {
        //     let topics =
        //         entries.iter().map(|e| &e.topic).collect::<HashSet<_>>();
        //
        //     let topics = topics.into_iter().cloned()
        //
        //         .map(|name| Topic{nam})
        //         .collect::<Vec<_>>();
        //
        //     TopicSequence::from_slice(&topics)
        // };

        // for entry in entries.iter() {
        //     let topic_id: i64 = sqlx::query!(
        //         r#"
        //             INSERT INTO topic (name)
        //             VALUES (?)
        //             ON CONFLICT(name) DO UPDATE SET name=excluded.name
        //             RETURNING id
        //         "#,
        //         entry.topic
        //     )
        //     .fetch_one(&pool)
        //     .await?
        //     .id;
        //
        //     // sqlx::query!(
        //     //     r#"
        //     //         INSERT INTO entry (id, topic_id, question, truth, added_at, reviewed_at, complexity)
        //     //         VALUES (?, ?, ?, ?, ?,)
        //     //     "#
        //     // ).execute(&pool)
        //     // .await?;
        // }
    }

    /// Fetch new knowledge and mutate internal state
    pub fn next_knowledge(&mut self) -> anyhow::Result<Entry> {
        assert!(self.topics.len() > 0);
        todo!()

        // let topic = {
        //     match self.topics.next() {
        //         Some(topic) => topic,
        //         None => {
        //             self.topics.reset();
        //
        //             self.topics.next().expect("Sequence is reset")
        //         }
        //     }
        // };
        //
        // let mut entries = self
        //     .entries
        //     .iter_mut()
        //     .filter(|e| e.topic == topic)
        //     .collect::<Vec<_>>();
        //
        // if entries.is_empty() {
        //     return Err(
        //         anyhow::anyhow!("No entries for topic: {}"),
        //         topic.name,
        //     );
        // }
        //
        // if let Some(entry) =
        //     entries.iter_mut().find(|e| e.reviewed_at.is_none())
        // {
        //     entry.reviewed_at = Some(chrono::Utc::now());
        //     return Ok(entry.clone());
        // }
        //
        // //todo: what do you think about switching topic instead using old one?
        // entries.iter_mut().for_each(|e| e.reviewed_at = None);
        //
        // entries[0].reviewed_at = Some(chrono::Utc::now());
        //
        // Ok(entries[0].clone())
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
