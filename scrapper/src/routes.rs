use std::sync::Arc;

use crate::config::Config;
use crate::model::Article;
use axum::{
    Json, Router, extract::State, http::StatusCode, response::IntoResponse,
    routing::get,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
}

pub fn create_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/news", get(get_news))
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn get_news(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let client = reqwest::Client::new();
    let mut articles = Vec::new();

    let urls: Vec<reqwest::Url> = state
        .config
        .rss_sources
        .iter()
        .flat_map(|source| source.urls.iter().cloned())
        .collect();

    for url in urls {
        match fetch_feed(&client, url.clone()).await {
            Ok(mut feed_articles) => articles.append(&mut feed_articles),
            Err(e) => eprintln!("Error fetching feed {}: {}", url, e),
        }
    }

    Json(articles)
}

async fn fetch_feed(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> Result<Vec<Article>, Box<dyn std::error::Error>> {
    let response = client.get(url).send().await?;
    let content = response.bytes().await?;
    let feed = feed_rs::parser::parse(content.as_ref())?;

    let articles = feed
        .entries
        .into_iter()
        .map(|entry| {
            let title = entry.title.map(|t| t.content).unwrap_or_default();
            let link = entry
                .links
                .first()
                .map(|l| l.href.clone())
                .unwrap_or_default();

            let summary = entry.summary.map(|s| s.content).unwrap_or_default();
            
            let content = entry
                .content
                .as_ref()
                .and_then(|c| {
                    let body = c.body.as_ref()?;
                    if c.content_type.subty().as_str() == "html" {
                        match html2text::from_read(body.as_bytes(), 120) {
                            Ok(text) => Some(text),
                            Err(e) => {
                                tracing::error!("Failed to convert HTML to text for entry '{}': {}", title, e);
                                Some(body.clone())
                            }
                        }
                    } else {
                        Some(body.clone())
                    }
                })
                .unwrap_or(summary);

            let authors = entry.authors.into_iter().map(|a| a.name).collect();

            Article {
                authors,
                title,
                link,
                content,
                ty: entry
                    .content
                    .map(|c| c.content_type.subty().to_string())
                    .unwrap_or_default(),
            }
        })
        .collect();

    Ok(articles)
}
