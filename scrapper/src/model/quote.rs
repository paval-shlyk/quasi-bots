#[derive(sqlx::FromRow, serde::Serialize, Clone, Debug)]
pub struct FamousQuote {
    pub id: i64,
    pub text: String,
    pub author: String,
    pub when_used: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(sqlx::FromRow, serde::Serialize, Clone, Debug)]
pub struct QuoteAuthor {
    pub name: String,
    pub quotes_count: u64,
}
