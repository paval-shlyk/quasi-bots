mod model;
mod routes;

use std::collections::HashSet;

pub use model::*;
pub use routes::*;

pub type Topic = String;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DataBase {
    pub entries: Vec<KnowledgeEntry>,
    pub topics: TopicSequence,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TopicSequence {
    topics: Vec<Topic>,
    ///index to start
    next_topic: usize,
}

impl TopicSequence {
    pub fn from_slice(topics: &[Topic]) -> Self {
        let set = HashSet::<Topic>::from_iter(topics.iter().cloned());

        assert_eq!(set.len(), topics.len(), "Duplicated topics are detected");

        Self {
            topics: topics.to_vec(),
            next_topic: 0,
        }
    }

    pub fn next(&mut self) -> Option<Topic> {
        if self.next_topic >= self.topics.len() {
            return None;
        }

        let unused_topics = &self.topics[self.next_topic..];
        let idx = rand::random_range(0..unused_topics.len());

        let topic = unused_topics[idx].clone();

        self.topics.swap(self.next_topic, idx + self.next_topic);
        self.next_topic += 1;

        Some(topic)
    }

    pub fn try_push(&mut self, topic: Topic) -> anyhow::Result<()> {
        let is_duplicate =
            self.topics.iter().find(|old| **old == topic).is_some();

        if is_duplicate {
            return Err(anyhow::anyhow!("Duplicated topic"));
        }

        Ok(())
    }
}

impl DataBase {
    pub async fn load(config: &crate::Config) -> Self {
        let raw_entries =
            tokio::fs::read_to_string(&config.knowledge.knowledge_file)
                .await
                .expect("Failed to load knowledge file");

        let entries: Vec<KnowledgeEntry> = serde_yaml::from_str(&raw_entries)
            .expect("Invalid YAML format for knowledge file");

        let topics = {
            let topics =
                entries.iter().map(|e| &e.topic).collect::<HashSet<_>>();

            let topics = topics.into_iter().cloned().collect::<Vec<_>>();

            TopicSequence::from_slice(&topics)
        };

        Self { entries, topics }
    }

    pub fn update(&mut self) {

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
