use std::sync::Arc;

use crate::Config;

#[derive(Clone)]
pub struct FinanceState {
    pub(crate) config: Arc<Config>,

    #[allow(unused)]
    pub(crate) pool: sqlx::SqlitePool,
}
