mod model;

use std::sync::Arc;

use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

use crate::routes::AppState;

use model::Article;

pub async fn post_chosen_topic(
    State(_): State<Arc<AppState>>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Topic added")
}

pub async fn get_chosen_topics(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let topics = state
        .config
        .rss_sources
        .iter()
        .map(|source| source.clone())
        .collect::<Vec<_>>();

    Json(topics)
}

pub async fn get_today_news(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
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
