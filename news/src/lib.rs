mod config;
mod llm;
mod model;
mod state;

use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

pub use config::*;
pub use model::*;
pub use state::*;

pub use llm::*;

pub async fn connect(config: Config) -> anyhow::Result<NewsState> {
    Ok(NewsState {
        gemini_api: llm::GeminiApi::connect(config.gemini_config.clone())
            .await?,
        config: std::sync::Arc::new(config),
    })
}

#[derive(Debug, Clone, Hash)]
pub struct Entry {
    pub title: String,
    pub source: String,
    pub authors: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/news-bank/topics",
    responses(
        (status = 501, description = "Not implemented")
    )
)]
pub async fn post_chosen_topic(
    State(_state): State<NewsState>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Topic added")
}

#[utoipa::path(
    get,
    path = "/news-bank/topics",
    responses(
        (status = 200, description = "Topics retrieved successfully", body = Vec<RssSource>)
    )
)]
pub async fn get_chosen_topics(
    State(state): State<NewsState>,
) -> impl IntoResponse {
    let topics = state.config.rss_sources.to_vec();

    Json(topics)
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct FetchedArticle {
    pub articles: Vec<Article>,
    pub topic: String,
}

#[utoipa::path(
    get,
    path = "/news-bank/today",
    responses(
        (status = 200, description = "Today's news retrieved successfully", body = Vec<FetchedArticle>)
    )
)]
pub async fn get_today_news(
    State(state): State<NewsState>,
) -> impl IntoResponse {
    //todo: metric to estimate time
    let time = std::time::Instant::now();

    let client = reqwest::Client::new();

    let sources = state.config.rss_sources.clone();

    let mut tasks = tokio::task::JoinSet::new();

    let gemini_api = state.gemini_api.clone();
    let next_task = move |client, topic: String, url: reqwest::Url| {
        let gemini_api = gemini_api.clone();

        async move {
            match fetch_feed(&client, url.clone(), gemini_api).await {
                Ok(articles) => Some(FetchedArticle { topic, articles }),
                Err(e) => {
                    tracing::warn!("Error fetching feed {}: {}", url, e);
                    None
                }
            }
        }
    };

    for source in sources {
        for url in source.urls {
            tasks.spawn(next_task(client.clone(), source.topic.clone(), url));
        }
    }

    let articles = tasks
        .join_all()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    tracing::info!(
        "Elapsed to fetch all articles: {:.2} ms",
        time.elapsed().as_millis()
    );

    Json(articles)
}

fn parse_feed(feed: feed_rs::model::Feed, api: llm::GeminiApi) -> Vec<Article> {
    //huge width to prevent line breaks in the middle of sentences
    const HTML_WIDTH: usize = 1_000_000;

    feed
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

            //by default, we are not believing
            //to the summary text
            let content = entry
                .content
                .and_then(|c| parse_body(c.content_type, c.body?))
                .and_then(|c| {
                    match api.summarize_blocking(&c) {
                        Ok(summary) => Some(summary),
                        Err(e) => {
                            tracing::error!("Failed to summarize content: {e:?}");
                            None
                        },
                    }
                })
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
        .collect()
}

async fn fetch_feed(
    client: &reqwest::Client,
    url: reqwest::Url,
    api: llm::GeminiApi,
) -> anyhow::Result<Vec<Article>> {
    let resp = client.get(url).send().await?;
    let content = resp.bytes().await?;
    let feed = feed_rs::parser::parse(content.as_ref())?;

    let (tx, rx) = tokio::sync::oneshot::channel();

    rayon::spawn(move || {
        let articles = parse_feed(feed, api);

        let _ = tx.send(articles);
    });

    let articles = rx.await.expect("Tx cannot die");

    Ok(articles)
}
