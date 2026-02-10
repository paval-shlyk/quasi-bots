#[derive(serde::Serialize, Clone, Debug)]
pub struct Article {
    pub title: String,
    pub authors: Vec<String>,
    pub link: String,

    /// Information about the article, e.g. a summary or the full content
    pub content: String,
    pub ty: String,
}
