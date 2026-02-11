#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TrackingAsset {
    pub symbol: String,
    pub added_at: chrono::DateTime<chrono::Utc>,
}
