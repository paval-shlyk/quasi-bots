#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    // Unique identifier for the entry, can be a UUID or any string
    pub id: String,
    pub topic: String,
    pub tags: Vec<String>,

    pub question: String,

    pub truth: String,
}
