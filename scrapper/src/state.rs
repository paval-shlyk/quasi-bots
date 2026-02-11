use std::sync::Arc;

use crate::Config;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: sqlx::SqlitePool,
    pub needs_more_quotes: Arc<tokio::sync::Notify>,
}
