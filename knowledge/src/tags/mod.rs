#[derive(Debug, serde::Serialize, utoipa::ToSchema, schemars::JsonSchema)]
pub struct TagList {
    pub tags: Vec<String>,
}

pub async fn fetch_tags(pool: &sqlx::SqlitePool) -> anyhow::Result<TagList> {
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

    Ok(TagList { tags })
}
