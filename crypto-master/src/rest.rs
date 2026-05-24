use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use communication::proto;
use serde::{Deserialize, Serialize};

use crate::performance::compute_performance;
use crate::worker_pool::WorkerPool;

pub type AppState = Arc<WorkerPool>;

pub fn router(pool: Arc<WorkerPool>) -> Router {
    Router::new()
        .route("/api/workers", get(list_workers))
        .route("/api/workers/{worker_id}/status", get(get_worker_status))
        .route(
            "/api/workers/{worker_id}/portfolio",
            get(get_worker_portfolio),
        )
        .route(
            "/api/workers/{worker_id}/settings",
            axum::routing::put(update_worker_settings),
        )
        .route(
            "/api/workers/{worker_id}/performance",
            get(get_performance_report),
        )
        .route("/api/performance", get(get_aggregate_performance))
        .route(
            "/api/workers/{worker_id}/trades/open",
            get(get_worker_open_trades),
        )
        .route(
            "/api/workers/{worker_id}/predictions/open",
            get(get_worker_open_predictions),
        )
        .route(
            "/api/workers/{worker_id}/trades/history",
            get(get_worker_trade_history),
        )
        .route(
            "/api/workers/{worker_id}/predictions/history",
            get(get_worker_prediction_history),
        )
        .with_state(pool)
}

// -- Error handling ----------------------------------------------------------

#[derive(Serialize)]
struct ApiError {
    error: String,
}

type ApiResult<T> = Result<Json<T>, (axum::http::StatusCode, Json<ApiError>)>;

fn grpc_to_http(status: tonic::Status) -> (axum::http::StatusCode, Json<ApiError>) {
    let http_code = match status.code() {
        tonic::Code::NotFound => axum::http::StatusCode::NOT_FOUND,
        tonic::Code::InvalidArgument => axum::http::StatusCode::BAD_REQUEST,
        tonic::Code::Unavailable => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        http_code,
        Json(ApiError {
            error: status.message().to_string(),
        }),
    )
}

// -- Handlers ----------------------------------------------------------------

async fn list_workers(State(pool): State<AppState>) -> ApiResult<proto::WorkerList> {
    let results = pool
        .for_each(|mut client| async move {
            let resp = client.get_status(proto::Empty {}).await?;
            Ok(resp.into_inner())
        })
        .await;

    let workers: Vec<proto::WorkerStatus> = results
        .into_iter()
        .filter_map(|(id, res)| match res {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(worker_id = %id, error = %e, "Failed to get status");
                None
            }
        })
        .collect();

    Ok(Json(proto::WorkerList { workers }))
}

async fn get_worker_status(
    State(pool): State<AppState>,
    Path(worker_id): Path<String>,
) -> ApiResult<proto::WorkerStatus> {
    pool.with_worker(&worker_id, |mut client| async move {
        let resp = client.get_status(proto::Empty {}).await?;
        Ok(Json(resp.into_inner()))
    })
    .await
    .map_err(grpc_to_http)
}

async fn get_worker_portfolio(
    State(pool): State<AppState>,
    Path(worker_id): Path<String>,
) -> ApiResult<proto::Portfolio> {
    pool.with_worker(&worker_id, |mut client| async move {
        let resp = client.get_portfolio(proto::Empty {}).await?;
        Ok(Json(resp.into_inner()))
    })
    .await
    .map_err(grpc_to_http)
}

async fn update_worker_settings(
    State(pool): State<AppState>,
    Path(worker_id): Path<String>,
    Json(settings): Json<proto::BotSettings>,
) -> ApiResult<proto::UpdateSettingsResponse> {
    pool.with_worker(&worker_id, |mut client| async move {
        let resp = client.update_settings(settings).await?;
        Ok(Json(resp.into_inner()))
    })
    .await
    .map_err(grpc_to_http)
}

async fn get_performance_report(
    State(pool): State<AppState>,
    Path(worker_id): Path<String>,
) -> ApiResult<proto::PerformanceReport> {
    pool.with_worker(&worker_id, |mut client| async move {
        let status = client.get_status(proto::Empty {}).await?.into_inner();
        let portfolio = client.get_portfolio(proto::Empty {}).await?.into_inner();
        let trades = client
            .get_trade_history(proto::HistoryRequest {
                limit: 1000,
                offset: 0,
            })
            .await?
            .into_inner()
            .records;
        let predictions = client
            .get_prediction_history(proto::HistoryRequest {
                limit: 1000,
                offset: 0,
            })
            .await?
            .into_inner()
            .records;

        let report = compute_performance(
            &status.worker_id,
            &status.llm_provider,
            &portfolio,
            &trades,
            &predictions,
        );
        Ok(Json(report))
    })
    .await
    .map_err(grpc_to_http)
}

async fn get_aggregate_performance(
    State(pool): State<AppState>,
) -> ApiResult<proto::AggregatePerformanceReport> {
    let worker_ids = pool.list_ids().await;
    let mut reports = Vec::new();

    for wid in &worker_ids {
        let result = pool
            .with_worker(wid, |mut client| async move {
                let status = client.get_status(proto::Empty {}).await?.into_inner();
                let portfolio = client.get_portfolio(proto::Empty {}).await?.into_inner();
                let trades = client
                    .get_trade_history(proto::HistoryRequest {
                        limit: 1000,
                        offset: 0,
                    })
                    .await?
                    .into_inner()
                    .records;
                let predictions = client
                    .get_prediction_history(proto::HistoryRequest {
                        limit: 1000,
                        offset: 0,
                    })
                    .await?
                    .into_inner()
                    .records;

                Ok(compute_performance(
                    &status.worker_id,
                    &status.llm_provider,
                    &portfolio,
                    &trades,
                    &predictions,
                ))
            })
            .await;

        match result {
            Ok(report) => reports.push(report),
            Err(e) => {
                tracing::warn!(worker_id = %wid, error = %e, "Failed to fetch performance");
            }
        }
    }

    let total_fleet_pnl: rust_decimal::Decimal = reports
        .iter()
        .filter_map(|r| r.total_pnl.parse::<rust_decimal::Decimal>().ok())
        .sum();

    let best_worker_id = reports
        .iter()
        .max_by_key(|r| {
            r.total_pnl
                .parse::<rust_decimal::Decimal>()
                .unwrap_or_default()
        })
        .map(|r| r.worker_id.clone())
        .unwrap_or_default();

    Ok(Json(proto::AggregatePerformanceReport {
        workers: reports,
        best_worker_id,
        total_fleet_pnl: total_fleet_pnl.to_string(),
    }))
}

#[derive(Deserialize)]
struct PaginationParams {
    #[serde(default = "default_limit")]
    limit: i32,
    #[serde(default)]
    offset: i32,
}

fn default_limit() -> i32 {
    50
}

async fn get_worker_open_trades(
    State(pool): State<AppState>,
    Path(worker_id): Path<String>,
) -> ApiResult<proto::OpenTradesResponse> {
    pool.with_worker(&worker_id, |mut client| async move {
        let resp = client.get_open_trades(proto::Empty {}).await?;
        Ok(Json(resp.into_inner()))
    })
    .await
    .map_err(grpc_to_http)
}

async fn get_worker_open_predictions(
    State(pool): State<AppState>,
    Path(worker_id): Path<String>,
) -> ApiResult<proto::OpenPredictionsResponse> {
    pool.with_worker(&worker_id, |mut client| async move {
        let resp = client.get_open_predictions(proto::Empty {}).await?;
        Ok(Json(resp.into_inner()))
    })
    .await
    .map_err(grpc_to_http)
}

async fn get_worker_trade_history(
    State(pool): State<AppState>,
    Path(worker_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> ApiResult<proto::TradeHistoryResponse> {
    pool.with_worker(&worker_id, |mut client| async move {
        let resp = client
            .get_trade_history(proto::HistoryRequest {
                limit: params.limit,
                offset: params.offset,
            })
            .await?;
        Ok(Json(resp.into_inner()))
    })
    .await
    .map_err(grpc_to_http)
}

async fn get_worker_prediction_history(
    State(pool): State<AppState>,
    Path(worker_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> ApiResult<proto::PredictionHistoryResponse> {
    pool.with_worker(&worker_id, |mut client| async move {
        let resp = client
            .get_prediction_history(proto::HistoryRequest {
                limit: params.limit,
                offset: params.offset,
            })
            .await?;
        Ok(Json(resp.into_inner()))
    })
    .await
    .map_err(grpc_to_http)
}
