#[derive(serde::Serialize, Clone, Debug, utoipa::ToSchema)]
pub struct Article {
    pub title: String,
    pub authors: Vec<String>,
    pub link: Option<String>,

    /// Information about the article, e.g. a summary or the full content
    pub content: String,
}
