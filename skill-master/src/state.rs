use std::sync::Arc;

use crate::Config;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: sqlx::SqlitePool,
    pub needs_more_quotes: Arc<tokio::sync::Notify>,

    pub knowledge_state: knowledge::KnowledgeState,
    pub finance_state: finance::FinanceState,
    pub news_state: news::NewsState,

    /// Structural dig for `GET /health` (alert evaluator). Always present;
    /// `running=false` when evaluator was not spawned.
    pub alert_evaluator_status: finance::AlertEvaluatorStatus,

    pub metrics_handle: telemetry::PrometheusHandle,
}
