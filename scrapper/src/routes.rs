use std::sync::Arc;

use crate::{config::Config, finance};
use crate::{news, quotes, search};
use anyhow::Context;
use axum::{
    Router,
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
        .route("/news-bank/today", get(news::get_today_news))
        .route(
            "/news-bank/topics",
            get(news::get_chosen_topics).post(news::post_chosen_topic),
        )
        .route("/search", get(search::get_search))
        .route("/quotes-bank/authors", get(quotes::get_known_authors))
        .route("/quotes-bank/next", post(quotes::post_next_unused_quote))
        .route("/market-tracker/report", get(finance::routes::get_report))
        // .route(
        //     "/market-tracker/recommendations",
        //     get(finance::routes::handler_recommendations),
        // )
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
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
