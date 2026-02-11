mod model;
mod routes;

pub use routes::*;

pub type Topic = String;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DataBase {
    pub entries: Vec<KnowledgeEntry>, // pub entries: dashmap::DashMap<Topic, KnowledgeEntry>,
}

impl DataBase {
    pub async fn load(config: &crate::Config) -> Self {
        let raw_entries =
            tokio::fs::read_to_string(&config.knowledge.knowledge_file)
                .await
                .expect("Failed to load knowledge file");

        let entries: Vec<KnowledgeEntry> = serde_yaml::from_str(&raw_entries)
            .expect("Invalid YAML format for knowledge file");

        Self { entries }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeEntry {
    /// Unique question identifier
    pub id: String,
    pub topic: String,
    pub tags: Vec<String>,

    /// Question itself
    pub question: String,

    pub truth: String,

    pub added_at: chrono::DateTime<chrono::Utc>,
    pub last_review: Option<chrono::DateTime<chrono::Utc>>,

    // alternative for SuperMemo-2
    pub hardness_factor: f64,
    // good point about when to review)
    pub next_review: Option<chrono::DateTime<chrono::Utc>>,
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
