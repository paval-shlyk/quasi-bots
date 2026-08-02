use std::sync::Arc;

use crate::{Config, portfolio};

#[derive(Clone)]
pub struct FinanceState {
    #[allow(unused)]
    pub(crate) config: Arc<Config>,

    pub(crate) pool: sqlx::SqlitePool,

    pub(crate) api: portfolio::RestClient,
}

impl FinanceState {
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    pub fn api(&self) -> &portfolio::RestClient {
        &self.api
    }
}
