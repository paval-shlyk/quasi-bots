mod model;
mod routes;

pub use routes::*;

pub type Topic = String;

pub struct DataBase {
    pub entries: dashmap::DashMap<Topic, KnowledgeEntry>,
}

pub struct KnowledgeEntry {
    /// Unique question identifier
    pub id: String,

    /// Question itself
    pub question: String,

    pub truth: String,

    pub added_at: chrono::DateTime<chrono::Utc>,
    pub last_review: Option<chrono::DateTime<chrono::Utc>>,

    // alternative for SuperMemo-2
    pub hardness_factor: f64,
    // good point about when to review)
    // pub next_review: Option<chrono::DateTime<chrono::Utc>>,
}
