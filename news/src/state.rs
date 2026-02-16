use crate::Config;

#[derive(Debug, Clone)]
pub struct NewsState {
    pub config: std::sync::Arc<Config>,
}
