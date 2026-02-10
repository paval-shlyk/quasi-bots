use std::sync::Arc;

use crate::config::Config;
use crate::model::Article;
use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
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
        .route("/topics", get(get_topics).post(add_topic))
        .route("/search", get(search_news))
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn add_topic(State(_): State<Arc<AppState>>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Topic added")
}

async fn get_topics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let topics = state
        .config
        .rss_sources
        .iter()
        .map(|source| source.clone())
        .collect::<Vec<_>>();

    Json(topics)
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
            Err(e) => tracing::warn!("Error fetching feed {}: {}", url, e),
        }
    }

    Json(articles)
}

async fn fetch_feed(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> anyhow::Result<Vec<Article>> {
    //huge width to prevent line breaks in the middle of sentences
    const HTML_WIDTH: usize = 1_000_000;

    //todo: run bert or other model to summarize the content or simply truncate it to a certain
    //length

    let response = client.get(url).send().await?;
    let content = response.bytes().await?;
    let feed = feed_rs::parser::parse(content.as_ref())?;

    let articles = feed
        .entries
        .into_iter()
        .map(|entry| {
            let title = entry.title.map(|t| t.content).unwrap_or_default();
            let link = entry.links.first().map(|l| l.href.clone());

            let parse_body = |content_type: mediatype::MediaTypeBuf, body: String| -> Option<String> {
                //fixme: process other content types, e.g. markdown
                if content_type.subty().as_str() == "html" {
                    match html2text::from_read(body.as_bytes(), HTML_WIDTH) {
                        Ok(text) => Some(text),
                        Err(e) => {
                            tracing::error!("Failed to convert HTML to text for entry '{}': {}", title, e);
                            Some(body.clone())
                        }
                    }
                } else {
                    Some(body)
                }
            };

            let parse_summary = || entry.summary.and_then(|s| parse_body(s.content_type, s.content));

            let content = entry
                .content
                .and_then(|c| parse_body(c.content_type, c.body?))
                .unwrap_or_else(|| {
                    let Some(summary) = parse_summary() else {
                        tracing::warn!("Entry '{}' has no content or summary", title);
                        return "".to_string();
                    };

                    summary
                });

            let authors = entry.authors.into_iter().map(|a| a.name).collect();

            Article {
                authors,
                title,
                link,
                content,
            }
        })
        .collect();

    Ok(articles)
}

#[derive(serde::Deserialize)]
pub struct SearchQuery {
    #[serde(alias = "q")]
    pub query: String,
}

#[derive(serde::Serialize)]
pub struct FetchedArticle {
    pub title: String,
    pub link: String,
    /// short description of the article, e.g. the snippet from google news
    pub snippet: String,
}

#[derive(serde::Serialize)]
pub struct KnowledgeGraph {
    pub description: String,
    pub source_url: String,
}

#[derive(serde::Serialize)]
pub struct SearchResult {
    pub answer: Option<String>,
    /// knowledge graph information about the search query, e.g. a short description of the topic
    /// or a list of related topics
    pub knowledge_graph: Option<KnowledgeGraph>,
    pub articles: Vec<FetchedArticle>,
}

async fn perform_search(
    client: &reqwest::Client,
    api_key: &str,
    query: &str,
) -> anyhow::Result<SearchResult> {
    //todo: support other search engines, e.g. bing, duckduckgo, etc.

    let url = format!(
        "https://serpapi.com/search?q={query}&engine={engine}&api_key={api_key}&num=10",
        engine = "google"
    );

    let results = client
        .get(&url)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let organic_results = results["organic_results"]
        .as_array()
        .with_context(|| "organic_results is not an array")?
        .clone();

    let articles = organic_results
        .into_iter()
        .map(|result| {
            let title =
                result["title"].as_str().unwrap_or_default().to_string();
            let link = result["link"].as_str().unwrap_or_default().to_string();
            let snippet =
                result["snippet"].as_str().unwrap_or_default().to_string();

            FetchedArticle {
                title,
                link,
                snippet,
            }
        })
        .collect();

    let parse_knowledge_graph = || {
        let graph = results["knowledge_graph"].as_object()?;

        let description = graph.get("description")?.as_str()?.to_string();
        let source_url = graph.get("source")?.as_str()?.to_string();

        Some(KnowledgeGraph {
            description,
            source_url,
        })
    };

    let parse_answer = || {
        let answer_box = results["answer_box"].as_object()?;

        let answer = answer_box.get("answer")?.as_str()?.to_string();

        Some(answer)
    };

    Ok(SearchResult {
        answer: parse_answer(),
        knowledge_graph: parse_knowledge_graph(),
        articles,
    })
}

async fn search_news(
    search: Query<SearchQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    const APP_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

    let api_key = &state.config.serp_api_key;
    let client = reqwest::Client::builder()
        .user_agent(APP_AGENT)
        .build()
        .expect("Failed to build HTTP client");

    match perform_search(&client, api_key, &search.query).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => {
            tracing::error!("Search failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Search failed").into_response()
        }
    }
}
