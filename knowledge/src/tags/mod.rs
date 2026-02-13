mod routes;

pub use routes::*;

pub async fn fetch_tags(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<String>> {
    let tags = sqlx::query!(
        r#"
                SELECT name 
                FROM tag
            "#
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.name)
    .collect::<Vec<_>>();

    Ok(tags)
}
