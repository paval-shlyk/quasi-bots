/// When affinity is set to 0, it means the user has no affinity for this topic/entry, and it will
/// be treated as if affinity is not set at all
mod routes;

pub use routes::*;

pub async fn set_topic_affinity(
    topic_id: u64,
    days: u32,
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<()> {
    let days = if days == 0 { None } else { Some(days as i64) };
    let topic_id = topic_id as i64;

    sqlx::query!(
        r#"
            UPDATE topic
            SET affinity_days = ?
            WHERE id = ?
        "#,
        days,
        topic_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn set_entry_affinity(
    name: String,
    days: u32,
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<()> {
    let days = if days == 0 { None } else { Some(days as i64) };

    sqlx::query!(
        r#"
            UPDATE entry
            SET affinity_days = ?
            WHERE name = ?
        "#,
        days,
        name
    )
    .execute(pool)
    .await?;

    Ok(())
}
