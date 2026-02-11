#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TrackingAsset {
    pub symbol: String,
    pub name: String,
}
