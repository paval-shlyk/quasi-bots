use std::sync::Arc;

use crate::{Config, investment};

#[derive(Clone)]
pub struct FinanceState {
    pub config: Arc<Config>,

    pub(crate) pool: sqlx::SqlitePool,

    pub(crate) api: investment::RestClient,
}

impl FinanceState {
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    pub fn api(&self) -> &investment::RestClient {
        &self.api
    }
}
