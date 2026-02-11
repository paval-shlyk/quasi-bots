use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use reqwest::StatusCode;

use crate::{finance::metrics::AnalysisConfig, routes::AppState};

#[derive(serde::Deserialize)]
pub struct AssetQuery {
    pub asset: String,
}

///Generate report for given
pub async fn get_symbol_report(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<AssetQuery>,
) -> impl IntoResponse {
    let config = AnalysisConfig::default();

    match super::metrics::analyze_asset(&query.asset, &config).await {
        Some(report) => (StatusCode::OK, Json(report)).into_response(),
        None => (StatusCode::BAD_REQUEST).into_response(),
    }
}

pub async fn get_tracking_assets() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn post_tracking_asset() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

// return news about hype assets
pub async fn get_market_recommendations() -> impl IntoResponse {
    match super::recommendations::get_full_market_report().await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => {
            tracing::warn!("Failed to fetch market recommendations: {e}");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch recommendations",
            )
                .into_response()
        }
    }
}
