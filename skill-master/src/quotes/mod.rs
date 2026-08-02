mod model;
pub mod sync_task;

use crate::AppState;

pub use model::*;

pub async fn fetch_known_authors(
    pool: &sqlx::SqlitePool,
) -> sqlx::Result<Vec<QuoteAuthor>> {
    sqlx::query_as!(
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
    .fetch_all(pool)
    .await
}

pub async fn fetch_next_unused_quote(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Option<FamousQuote>> {
    let mut tx = pool.begin().await?;

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

pub fn notify_needs_more_quotes(state: &AppState) {
    state.needs_more_quotes.notify_one();
}
