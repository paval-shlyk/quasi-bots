#[derive(serde::Serialize, Clone, Debug, schemars::JsonSchema)]
pub struct FeedArticle {
    pub title: String,
    pub authors: Vec<String>,
    pub links: Vec<String>,
    /// if no date is available from [[crate::RawArticle]],
    /// then current time is used
    pub published_at: chrono::DateTime<chrono::Utc>,

    /// Information about the article, e.g. a summary or the full content
    pub content: String,
}

//fully parsed article stored in database
#[derive(Debug, sqlx::FromRow)]
pub struct SavedArticle {
    pub topic: String,

    pub title: String,
    pub content: String,

    pub authors: sqlx::types::Json<Vec<String>>,
    pub links: sqlx::types::Json<Vec<String>>,
    pub published_at: chrono::DateTime<chrono::Utc>,
}

impl SavedArticle {
    pub fn into_feed(self) -> FeedArticle {
        FeedArticle {
            title: self.title,
            authors: self.authors.0,
            links: self.links.0,
            published_at: self.published_at,
            content: self.content,
        }
    }
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct ArticlesWithTopic {
    pub articles: Vec<FeedArticle>,
    pub topic: String,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct TodayNews {
    pub topics: Vec<ArticlesWithTopic>,
}

//not fully parsed
#[derive(Debug)]
pub struct RawArticle {
    pub title: String,
    pub authors: Vec<String>,
    pub links: Vec<String>,

    pub content: Option<String>,
    pub summary: Option<String>,

    pub published_at: chrono::DateTime<chrono::Utc>,
}

impl RawArticle {
    /// try to summarize article content using LLM API
    pub async fn summarize(
        self,
        api: &crate::GeminiApi,
    ) -> anyhow::Result<FeedArticle> {
        let RawArticle {
            title,
            authors,
            links,
            mut content,
            summary,
            published_at,
        } = self;

        let summarized_content: Option<String>;

        //by default, we are not believing
        //to the summary text
        if let Some(text) = content.take() {
            summarized_content = api.summarize(&text).await?.into();
        } else {
            //if no content available then use summary information
            summarized_content = summary;
        }

        Ok(FeedArticle {
            published_at,
            title,
            authors,
            links,
            content: summarized_content.unwrap_or_default(),
        })
    }

    pub fn into_article_unchecked(self) -> FeedArticle {
        let content = if let Some(text) = self.content {
            Some(text)
        } else {
            self.summary
        }
        .unwrap_or_default();

        FeedArticle {
            title: self.title,
            authors: self.authors,
            links: self.links,
            published_at: self.published_at,
            content,
        }
    }
}
