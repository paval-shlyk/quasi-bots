use std::sync::Arc;

use crate::model::Article;
use crate::quotes;
use crate::{config::Config, finance};
use anyhow::Context;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: sqlx::SqlitePool,
    pub needs_more_quotes: Arc<tokio::sync::Notify>,
}

pub fn create_routes(state: Arc<AppState>) -> Router {
    tokio::task::spawn(quote_sync_task(state.clone()));

    Router::new()
        .route("/health", get(health_check))
        .route("/news", get(get_news))
        .route("/topics", get(get_topics).post(add_topic))
        .route("/search", get(crate::search::search_news))
        .route("/quotes-bank/authors", get(quotes::get_known_authors))
        .route("/quotes-bank/next", post(quotes::post_next_unused_quote))
        .route(
            "/market-tracker/report",
            get(finance::routes::get_symbol_report),
        )
        // .route(
        //     "/market-tracker/recommendations",
        //     get(finance::routes::handler_recommendations),
        // )
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

async fn quote_sync_task(state: Arc<AppState>) {
    #[derive(serde::Deserialize)]
    struct ZenQuote {
        q: String,
        a: String,
    }

    async fn insert_new_quotes(
        quotes: Vec<ZenQuote>,
        pool: &sqlx::SqlitePool,
    ) -> anyhow::Result<()> {
        for quote in quotes {
            let author_name = quote.a.trim();
            let quote_text = quote.q.trim();

            let mut tx = pool.begin().await?;

            // Insert author if not exists
            let author_id: i64 = sqlx::query!(
                r#"
                    INSERT INTO author (name)
                    VALUES (?)
                    ON CONFLICT(name) DO UPDATE SET name=excluded.name
                    RETURNING id
                "#,
                author_name
            )
            .fetch_one(tx.as_mut())
            .await
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to insert author '{}' into DB: {}",
                    author_name,
                    e
                )
            })?
            .id;

            let already_exists = sqlx::query!(
                r#"
                SELECT id FROM quote
                WHERE text = ? AND author_id = ?
                "#,
                quote_text,
                author_id
            )
            .fetch_optional(tx.as_mut())
            .await?
            .is_some();

            if already_exists {
                tracing::info!(
                    "Quote already exists in DB, skipping: '{}'",
                    quote_text
                );
                continue;
            }

            sqlx::query!(
                r#"
                INSERT INTO quote (text, author_id)
                VALUES (?, ?)
                "#,
                quote_text,
                author_id
            )
            .execute(tx.as_mut())
            .await?;

            tx.commit().await?;
        }

        Ok(())
    }

    async fn fetch_quotes_from_api() -> anyhow::Result<Vec<ZenQuote>> {
        let client = reqwest::Client::new();

        let quotes = client
            .get("https://zenquotes.io/api/quotes")
            .send()
            .await
            .with_context(|| "Failed to fetch quote from external API")?
            .json::<Vec<ZenQuote>>()
            .await
            .with_context(|| "Failed to parse quote from external API")?;

        Ok(quotes)
    }

    const MIN_FRESH_QUOTES: u64 = 10;

    loop {
        let count: u64 = sqlx::query!(
            r#"
            SELECT COUNT(id) as "count: u64"
            FROM quote
            WHERE when_used IS NULL OR when_used < datetime('now', '-6 months')
            "#
        )
        .fetch_one(&state.pool)
        .await
        .map(|row| row.count)
        .unwrap_or_default();

        if count > MIN_FRESH_QUOTES {
            tracing::info!(
                "Found {} fresh quotes in DB, skipping fetch from external source",
                count
            );

            tokio::select! {
                _ = state.needs_more_quotes.notified() => {
                    tracing::info!("Received notification for more quotes, fetching from external source");
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(4 * 60 * 60)) => {}
            };

            continue;
        }

        tracing::info!(
            "Only {} fresh quotes available in DB, fetching more from external source",
            count
        );

        let quotes = fetch_quotes_from_api()
            .await
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to fetch quotes from external API: {}",
                    e
                )
            })
            .unwrap_or_default();

        let _ = insert_new_quotes(quotes, &state.pool)
            .await
            .inspect_err(|e| tracing::error!("Failed to insert quotes: {}", e));
    }
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
