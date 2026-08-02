mod affinity;
mod entries;
mod reviews;
mod state;
mod tags;
mod topics;

pub use affinity::*;
use anyhow::Context;
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

/// # Arguments
/// * `state` - The current knowledge state, which includes the database connection pool.
/// * `files` - The path to the YAML file containing the knowledge entries.
///   That's also possible to provide a directory, in that case all YAML files in the directory will
///   be loaded and merged into the database.
pub async fn refresh_from_files(
    state: &KnowledgeState,
    files: &std::path::Path,
) -> anyhow::Result<()> {
    async fn load_file(
        path: &std::path::Path,
    ) -> anyhow::Result<Vec<HumanEntry>> {
        let raw_entries =
            tokio::fs::read_to_string(path).await.inspect_err(|e| {
                tracing::warn!(
                    "Failed to load knowledge file {}: {}",
                    path.display(),
                    e
                )
            })?;

        let entries: Vec<HumanEntry> = serde_yaml::from_str(&raw_entries)
            .inspect_err(|e| {
                tracing::warn!(
                    "Invalid YAML format for knowledge file {}: {}",
                    path.display(),
                    e
                )
            })?;

        Ok(entries)
    }

    let file_iter = walkdir::WalkDir::new(files)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        });

    let entries = {
        let mut entries = Vec::new();

        for file in file_iter {
            let Ok(entries_from_file) = load_file(file.path()).await else {
                tracing::warn!(
                    "Skipping file {} due to previous errors",
                    file.path().display()
                );
                continue;
            };

            entries.extend(entries_from_file);
        }

        entries
    };

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

    let mut tx = state.pool.begin().await?;

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
        .fetch_one(tx.as_mut())
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
        .fetch_one(tx.as_mut())
        .await
        .with_context(|| {
            format!("Failed to insert entry '{}' into the database", entry.id)
        })?
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
                    .fetch_one(tx.as_mut())
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
            .execute(tx.as_mut())
            .await?;
        }
    }

    tx.commit().await?;

    Ok(())
}
