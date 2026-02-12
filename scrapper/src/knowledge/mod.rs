mod model;
mod routes;
mod topic_sequence;

use std::collections::HashSet;

pub use model::*;
pub use routes::*;
use topic_sequence::TopicSequence;

pub type Topic = String;

#[derive(Debug)]
pub struct DataBase {
    entries: Vec<KnowledgeEntry>,
    topics: TopicSequence,
    pool: sqlx::SqlitePool,
}

impl DataBase {
    pub async fn load(
        config: &crate::Config,
        pool: sqlx::SqlitePool,
    ) -> anyhow::Result<Self> {
        let raw_entries =
            tokio::fs::read_to_string(&config.knowledge.knowledge_file)
                .await
                .expect("Failed to load knowledge file");

        //todo: drop tables somehow

        let entries: Vec<KnowledgeEntry> = serde_yaml::from_str(&raw_entries)
            .expect("Invalid YAML format for knowledge file");

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

        let topics = {
            let topics =
                entries.iter().map(|e| &e.topic).collect::<HashSet<_>>();

            let topics = topics.into_iter().cloned().collect::<Vec<_>>();

            TopicSequence::from_slice(&topics)
        };

        Ok(Self {
            topics,
            entries,
            pool,
        })
    }

    /// Fetch new knowledge and mutate internal state
    pub fn next_knowledge(&mut self) -> anyhow::Result<KnowledgeEntry> {
        assert!(self.topics.len() > 0);

        let topic = {
            match self.topics.next() {
                Some(topic) => topic,
                None => {
                    self.topics.reset();

                    self.topics.next().expect("Sequence is reset")
                }
            }
        };

        let mut entries = self
            .entries
            .iter_mut()
            .filter(|e| e.topic == topic)
            .collect::<Vec<_>>();

        if entries.is_empty() {
            return Err(anyhow::anyhow!("No entries for topic: {topic}"));
        }

        if let Some(entry) =
            entries.iter_mut().find(|e| e.reviewed_at.is_none())
        {
            entry.reviewed_at = Some(chrono::Utc::now());
            return Ok(entry.clone());
        }

        //todo: what do you think about switching topic instead using old one?
        entries.iter_mut().for_each(|e| e.reviewed_at = None);

        entries[0].reviewed_at = Some(chrono::Utc::now());

        Ok(entries[0].clone())
    }
}

// fn serialize_entries<S>(
//     entries: &dashmap::DashMap<Topic, KnowledgeEntry>,
//     serializer: S,
// ) -> Result<S::Ok, S::Error>
// where
//     S: serde::Serializer,
// {
//     use serde::Serialize;
//     use std::collections::BTreeMap;
//
//     let map: BTreeMap<_, _> = entries
//         .iter()
//         .map(|entry| (entry.key().clone(), entry.value().clone()))
//         .collect();
//
//     map.serialize(serializer)
// }
//
// fn deserialize_entries<'de, D>(
//     deserializer: D,
// ) -> Result<dashmap::DashMap<Topic, KnowledgeEntry>, D::Error>
// where
//     D: serde::Deserializer<'de>,
// {
//     use serde::Deserialize;
//     use std::collections::HashMap;
//
//     let map: HashMap<Topic, KnowledgeEntry> =
//         HashMap::deserialize(deserializer)?;
//
//     Ok(map.into_iter().collect())
// }
