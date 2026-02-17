use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{Config, links::BrokenLink};

#[derive(Clone)]
pub struct NewsState {
    pub config: Arc<Config>,
    pub gemini_api: crate::llm::GeminiApi,
    pub pool: sqlx::SqlitePool,

    //the list of recently broken links
    pub broken_links: Arc<RwLock<Vec<BrokenLink>>>,

    pub purge_notify: Arc<tokio::sync::Notify>,
}
