#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct Entry {
    /// Unique question identifier
    pub id: String,
    pub topic: String,
    pub tags: Vec<String>,

    /// Question itself
    pub question: String,

    pub truth: String,

    pub added_at: chrono::DateTime<chrono::Utc>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,

    /// value from 0 to 100
    pub complexity: u16,
}
