use crate::Config;

#[derive(Clone)]
pub struct NewsState {
    pub config: std::sync::Arc<Config>,
    pub gemini_api: crate::llm::GeminiApi,
    pub pool: sqlx::SqlitePool,
}
