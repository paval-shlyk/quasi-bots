mod affinity;
mod entries;
mod reviews;
mod state;
mod tags;
mod topics;

pub use affinity::*;
pub use entries::*;
pub use reviews::*;
pub use state::*;
pub use tags::*;
pub use topics::*;

#[derive(Debug, Clone)]
pub enum KnowledgeMode {
    WithTag { tag: String },
    Random,
}

pub async fn connect(pool: sqlx::SqlitePool) -> anyhow::Result<KnowledgeState> {
    Ok(KnowledgeState { pool })
}

pub async fn refresh_from_files(
    state: &KnowledgeState,
    file: &std::path::Path,
) -> anyhow::Result<()> {
    let raw_entries = tokio::fs::read_to_string(file)
        .await
        .expect("Failed to load knowledge file");

    let entries: Vec<HumanEntry> = serde_yaml::from_str(&raw_entries)
        .expect("Invalid YAML format for knowledge file");

    sqlx::query!(
        r#"
                DELETE FROM m2m_entry_tag;
                DELETE FROM entry;
                DELETE FROM tag;
                DELETE FROM topic;
            "#
    )
    .execute(&state.pool)
    .await?;

    for entry in entries.iter() {
        let topic_id: i64 = sqlx::query!(
            r#"
                        INSERT INTO topic (name)
                        VALUES (?)
                        ON CONFLICT(name) DO UPDATE SET name=excluded.name
                        RETURNING id
                    "#,
            entry.topic
        )
        .fetch_one(&state.pool)
        .await?
        .id;

        let entry_id = sqlx::query!(
            r#"
                        INSERT INTO entry (topic_id, name, question, truth)
                        VALUES (?, ?, ?, ?)
                        RETURNING id
                    "#,
            topic_id,
            entry.id,
            entry.question,
            entry.truth
        )
        .fetch_one(&state.pool)
        .await?
        .id;

        for tag in entry.tags.iter() {
            let tag_id: i64 = sqlx::query!(
                        r#"
                                INSERT INTO tag (name)
                                VALUES (?)
                                ON CONFLICT(name) DO UPDATE SET name=excluded.name
                                RETURNING id
                            "#,
                        tag
                    )
                    .fetch_one(&state.pool)
                    .await?
                    .id;

            sqlx::query!(
                r#"
                        INSERT INTO m2m_entry_tag (entry_id, tag_id)
                        VALUES (?, ?)
                    "#,
                entry_id,
                tag_id
            )
            .execute(&state.pool)
            .await?;
        }
    }

    Ok(())
}
