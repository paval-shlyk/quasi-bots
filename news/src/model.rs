#[derive(serde::Serialize, Clone, Debug, utoipa::ToSchema)]
pub struct Article {
    pub title: String,
    pub authors: Vec<String>,
    pub links: Vec<String>,

    /// Information about the article, e.g. a summary or the full content
    pub content: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct FetchedArticle {
    pub articles: Vec<Article>,
    pub topic: String,
}
