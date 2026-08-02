use std::sync::Arc;

use communication::proto;
use tonic::{Request, Response, Status};

use crate::performance::compute_performance;
use crate::worker_pool::WorkerPool;

pub struct MasterGrpcServer {
    pool: Arc<WorkerPool>,
}

impl MasterGrpcServer {
    pub fn new(pool: Arc<WorkerPool>) -> Self {
        Self { pool }
    }
}

#[tonic::async_trait]
impl communication::MasterService for MasterGrpcServer {
    async fn list_workers(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::WorkerList>, Status> {
        let results = self
            .pool
            .for_each(|mut client| async move {
                let resp = client.get_status(proto::Empty {}).await?;
                Ok(resp.into_inner())
            })
            .await;

        let workers: Vec<proto::WorkerStatus> = results
            .into_iter()
            .filter_map(|(id, res)| match res {
                Ok(status) => Some(status),
                Err(e) => {
                    tracing::warn!(worker_id = %id, error = %e, "Failed to get worker status");
                    None
                }
            })
            .collect();

        Ok(Response::new(proto::WorkerList { workers }))
    }

    async fn get_worker_status(
        &self,
        req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::WorkerStatus>, Status> {
        let worker_id = req.into_inner().id;
        self.pool
            .with_worker(&worker_id, |mut client| async move {
                let resp = client.get_status(proto::Empty {}).await?;
                Ok(Response::new(resp.into_inner()))
            })
            .await
    }

    async fn get_worker_portfolio(
        &self,
        req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::Portfolio>, Status> {
        let worker_id = req.into_inner().id;
        self.pool
            .with_worker(&worker_id, |mut client| async move {
                let resp = client.get_portfolio(proto::Empty {}).await?;
                Ok(Response::new(resp.into_inner()))
            })
            .await
    }

    async fn update_worker_settings(
        &self,
        req: Request<proto::UpdateWorkerSettingsRequest>,
    ) -> Result<Response<proto::UpdateSettingsResponse>, Status> {
        let inner = req.into_inner();
        let worker_id = inner.worker_id;
        let settings = inner
            .settings
            .ok_or_else(|| Status::invalid_argument("Missing settings"))?;

        self.pool
            .with_worker(&worker_id, |mut client| async move {
                let resp = client.update_settings(settings).await?;
                Ok(Response::new(resp.into_inner()))
            })
            .await
    }

    async fn get_performance_report(
        &self,
        req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::PerformanceReport>, Status> {
        let worker_id = req.into_inner().id;

        self.pool
            .with_worker(&worker_id, |mut client| async move {
                let status =
                    client.get_status(proto::Empty {}).await?.into_inner();
                let portfolio =
                    client.get_portfolio(proto::Empty {}).await?.into_inner();
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

                Ok(Response::new(report))
            })
            .await
    }

    async fn get_aggregate_performance(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::AggregatePerformanceReport>, Status> {
        let worker_ids = self.pool.list_ids().await;

        let mut reports = Vec::new();
        for wid in &worker_ids {
            let result = self
                .pool
                .with_worker(wid, |mut client| async move {
                    let status =
                        client.get_status(proto::Empty {}).await?.into_inner();
                    let portfolio = client
                        .get_portfolio(proto::Empty {})
                        .await?
                        .into_inner();
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

        Ok(Response::new(proto::AggregatePerformanceReport {
            workers: reports,
            best_worker_id,
            total_fleet_pnl: total_fleet_pnl.to_string(),
        }))
    }

    async fn get_worker_open_trades(
        &self,
        req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::OpenTradesResponse>, Status> {
        let worker_id = req.into_inner().id;
        self.pool
            .with_worker(&worker_id, |mut client| async move {
                let resp = client.get_open_trades(proto::Empty {}).await?;
                Ok(Response::new(resp.into_inner()))
            })
            .await
    }

    async fn get_worker_open_predictions(
        &self,
        req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::OpenPredictionsResponse>, Status> {
        let worker_id = req.into_inner().id;
        self.pool
            .with_worker(&worker_id, |mut client| async move {
                let resp = client.get_open_predictions(proto::Empty {}).await?;
                Ok(Response::new(resp.into_inner()))
            })
            .await
    }

    async fn get_worker_trade_history(
        &self,
        req: Request<proto::WorkerHistoryRequest>,
    ) -> Result<Response<proto::TradeHistoryResponse>, Status> {
        let inner = req.into_inner();
        self.pool
            .with_worker(&inner.worker_id, |mut client| async move {
                let resp = client
                    .get_trade_history(proto::HistoryRequest {
                        limit: inner.limit,
                        offset: inner.offset,
                    })
                    .await?;
                Ok(Response::new(resp.into_inner()))
            })
            .await
    }

    async fn get_worker_prediction_history(
        &self,
        req: Request<proto::WorkerHistoryRequest>,
    ) -> Result<Response<proto::PredictionHistoryResponse>, Status> {
        let inner = req.into_inner();
        self.pool
            .with_worker(&inner.worker_id, |mut client| async move {
                let resp = client
                    .get_prediction_history(proto::HistoryRequest {
                        limit: inner.limit,
                        offset: inner.offset,
                    })
                    .await?;
                Ok(Response::new(resp.into_inner()))
            })
            .await
    }
}
