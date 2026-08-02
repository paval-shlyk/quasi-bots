#[derive(
    sqlx::FromRow, serde::Serialize, Clone, Debug, schemars::JsonSchema,
)]
pub struct FamousQuote {
    pub id: i64,
    pub text: String,
    pub author: String,
    pub when_used: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(
    sqlx::FromRow, serde::Serialize, Clone, Debug, schemars::JsonSchema,
)]
pub struct QuoteAuthor {
    pub name: String,
    pub quotes_count: u64,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct QuoteAuthorList {
    pub authors: Vec<QuoteAuthor>,
}

impl From<Vec<QuoteAuthor>> for QuoteAuthorList {
    fn from(authors: Vec<QuoteAuthor>) -> Self {
        Self { authors }
    }
}
