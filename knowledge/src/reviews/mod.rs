mod routes;

pub use routes::*;

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Review {
    pub id: i64,
    pub entry_name: String,
    pub reviewed_at: chrono::DateTime<chrono::Utc>,
    pub attempts: i64,
}

pub async fn fetch_recent_reviews(
    pool: &sqlx::SqlitePool,
    days: Option<u32>,
) -> anyhow::Result<Vec<Review>> {
    let days = days.unwrap_or(7) as i32;

    let reviews = sqlx::query_as!(
        Review,
        r#"
            SELECT
                r.id,
                e.name AS entry_name,
                r.reviewed_at as "reviewed_at: chrono::DateTime<chrono::Utc>",
                r.attempts
            FROM review as r
            JOIN entry as e ON r.entry_id = e.id
            WHERE r.reviewed_at > datetime('now', '-' || ? || ' days')
        "#,
        days
    )
    .fetch_all(pool)
    .await?;

    Ok(reviews)
}

pub async fn update_review(
    pool: &sqlx::SqlitePool,
    entry_name: String,
    attempts: i32,
) -> anyhow::Result<()> {
    //only one review per entry, so we can just update it by entry name
    sqlx::query!(
        r#"
            UPDATE review
            SET attempts = ?
            WHERE 
                entry_id IN (SELECT id FROM entry WHERE name = ?)
        "#,
        attempts,
        entry_name
    )
    .execute(pool)
    .await?;

    Ok(())
}
