mod model;
pub mod sync_task;

use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

use crate::AppState;

pub use model::*;

pub async fn get_known_authors(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let authors = sqlx::query_as!(
        QuoteAuthor,
        r#"
            SELECT name, quotes_count as "quotes_count: u64"
            FROM (
                SELECT
                    a.name as name, 
                    COUNT(q.id) as quotes_count
                FROM author as a
                LEFT JOIN quote as q ON a.id = q.author_id
                GROUP BY a.id
            )
            WHERE quotes_count > 0
	    ORDER BY quotes_count DESC
        "#
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    Json(authors)
}

pub async fn post_next_unused_quote(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match fetch_famous_quote(&state.pool).await {
        Ok(maybe_quote) => match maybe_quote {
            Some(quote) => (StatusCode::OK, Json(quote)).into_response(),
            None => {
                state.needs_more_quotes.notify_one();
                (
                    StatusCode::NOT_FOUND,
                    "No fresh quotes available, please try again later",
                )
                    .into_response()
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch quote: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch quote")
                .into_response()
        }
    }
}

async fn fetch_famous_quote(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Option<FamousQuote>> {
    let mut tx = pool.begin().await?;

    // Try to get a quote from DB that hasn't been used in 6 months or ever
    let maybe_quote = sqlx::query_as!(
        FamousQuote,
        r#"
        SELECT q.id, q.text, a.name as author, q.when_used as "when_used: chrono::DateTime<chrono::Utc>"
        FROM quote q
        JOIN author a ON q.author_id = a.id
        WHERE q.when_used IS NULL OR q.when_used < datetime('now', '-6 months')
        LIMIT 1
        "#
    )
    .fetch_optional(tx.as_mut())
    .await?;

    match maybe_quote {
        Some(quote) => {
            // Mark as used
            sqlx::query!(
                r#"
            UPDATE quote
            SET when_used = datetime('now')
            WHERE id = ?
            "#,
                quote.id
            )
            .execute(tx.as_mut())
            .await?;

            tx.commit().await?;

            Ok(Some(quote))
        }
        None => Ok(None),
    }
}
