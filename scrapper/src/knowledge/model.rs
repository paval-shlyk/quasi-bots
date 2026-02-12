/// Human readable entry for a question
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HumanEntry {
    // Unique identifier for the entry, can be a UUID or any string
    pub id: String,
    pub topic: String,
    pub tags: Vec<String>,

    pub question: String,

    pub truth: String,

    /// Number of days until the next review, if the user has an affinity for this topic/entry,
    /// otherwise None
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub affinity_days: Option<u32>,
}
