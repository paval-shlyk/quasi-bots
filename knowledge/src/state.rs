#[derive(Debug, Clone)]
pub struct KnowledgeState {
    pub(crate) pool: sqlx::SqlitePool,
}
