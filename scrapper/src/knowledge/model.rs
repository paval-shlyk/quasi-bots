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
