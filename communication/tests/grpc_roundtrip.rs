use communication::proto;
use communication::{WorkerService, WorkerServiceServer};
use communication::{MasterService, MasterServiceServer};
use communication::WorkerServiceClient;
use communication::MasterServiceClient;
use prost_types::Timestamp;
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status};


// ---------------------------------------------------------------------------
// Mock WorkerService
// ---------------------------------------------------------------------------

struct MockWorker;

#[tonic::async_trait]
impl WorkerService for MockWorker {
    async fn get_status(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::WorkerStatus>, Status> {
        Ok(Response::new(proto::WorkerStatus {
            worker_id: "test-worker".into(),
            llm_provider: "grok".into(),
            trading_active: true,
            polymarket_active: true,
            started_at: Some(Timestamp { seconds: 1000, nanos: 0 }),
            last_heartbeat: Some(Timestamp { seconds: 2000, nanos: 0 }),
            open_trade_count: 3,
            open_prediction_count: 2,
        }))
    }

    async fn get_portfolio(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::Portfolio>, Status> {
        Ok(Response::new(proto::Portfolio {
            total_balance: "10000".into(),
            available_balance: "8000".into(),
            trading_allocated: "1500".into(),
            polymarket_allocated: "500".into(),
            unrealized_pnl: "200".into(),
            realized_pnl: "350".into(),
            base_currency: "USDT".into(),
        }))
    }

    async fn update_settings(
        &self,
        req: Request<proto::BotSettings>,
    ) -> Result<Response<proto::UpdateSettingsResponse>, Status> {
        let settings = req.into_inner();
        Ok(Response::new(proto::UpdateSettingsResponse {
            success: true,
            message: "applied".into(),
            applied_settings: Some(settings),
        }))
    }

    async fn get_open_trades(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::OpenTradesResponse>, Status> {
        Ok(Response::new(proto::OpenTradesResponse {
            positions: vec![proto::OpenPosition {
                id: "pos-1".into(),
                pair: "BTC/USDT".into(),
                side: "long".into(),
                entry_price: "50000".into(),
                quantity: "0.1".into(),
                current_price: "51000".into(),
                unrealized_pnl: "100".into(),
                stop_loss_price: "49000".into(),
                take_profit_price: "55000".into(),
                allocated_capital: "5000".into(),
                opened_at: Some(Timestamp { seconds: 1000, nanos: 0 }),
                updated_at: Some(Timestamp { seconds: 2000, nanos: 0 }),
            }],
        }))
    }

    async fn get_open_predictions(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::OpenPredictionsResponse>, Status> {
        Ok(Response::new(proto::OpenPredictionsResponse {
            predictions: vec![proto::OpenPrediction {
                id: "pred-1".into(),
                market_id: "mkt-abc".into(),
                market_title: "Will BTC hit 100k?".into(),
                side: "yes".into(),
                shares: "50".into(),
                avg_price: "0.6".into(),
                current_price: "0.65".into(),
                unrealized_pnl: "2.5".into(),
                allocated_capital: "30".into(),
                opened_at: Some(Timestamp { seconds: 1500, nanos: 0 }),
                updated_at: Some(Timestamp { seconds: 2500, nanos: 0 }),
            }],
        }))
    }

    async fn get_trade_history(
        &self,
        req: Request<proto::HistoryRequest>,
    ) -> Result<Response<proto::TradeHistoryResponse>, Status> {
        let params = req.into_inner();
        let count = if params.limit > 0 { params.limit.min(2) } else { 1 };
        let records = (0..count)
            .map(|i| proto::TradeRecord {
                id: format!("trade-{i}"),
                pair: "ETH/USDT".into(),
                side: "long".into(),
                order_type: "market".into(),
                quantity: "1".into(),
                price: "3000".into(),
                filled_quantity: "1".into(),
                avg_fill_price: "3001".into(),
                fee: "3".into(),
                status: "filled".into(),
                llm_rationale: "momentum".into(),
                llm_confidence: "0.85".into(),
                created_at: Some(Timestamp { seconds: 1000 + i as i64, nanos: 0 }),
                updated_at: Some(Timestamp { seconds: 1001 + i as i64, nanos: 0 }),
            })
            .collect();
        Ok(Response::new(proto::TradeHistoryResponse { records }))
    }

    async fn get_prediction_history(
        &self,
        req: Request<proto::HistoryRequest>,
    ) -> Result<Response<proto::PredictionHistoryResponse>, Status> {
        let params = req.into_inner();
        let count = if params.limit > 0 { params.limit.min(2) } else { 1 };
        let records = (0..count)
            .map(|i| proto::PredictionRecord {
                id: format!("predrec-{i}"),
                market_id: "mkt-xyz".into(),
                market_title: "Election outcome".into(),
                side: "no".into(),
                action: "buy".into(),
                shares: "20".into(),
                price_per_share: "0.4".into(),
                total_cost: "8".into(),
                status: "filled".into(),
                resolution: "pending".into(),
                llm_rationale: "polling data".into(),
                llm_confidence: "0.7".into(),
                created_at: Some(Timestamp { seconds: 2000 + i as i64, nanos: 0 }),
                updated_at: Some(Timestamp { seconds: 2001 + i as i64, nanos: 0 }),
            })
            .collect();
        Ok(Response::new(proto::PredictionHistoryResponse { records }))
    }
}


// ---------------------------------------------------------------------------
// Mock MasterService (delegates to a single mock worker for simplicity)
// ---------------------------------------------------------------------------

struct MockMaster {
    worker_addr: String,
}

impl MockMaster {
    async fn worker_client(&self) -> Result<WorkerServiceClient<Channel>, Status> {
        WorkerServiceClient::connect(self.worker_addr.clone())
            .await
            .map_err(|e| Status::unavailable(format!("Cannot reach worker: {e}")))
    }
}

#[tonic::async_trait]
impl MasterService for MockMaster {
    async fn list_workers(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::WorkerList>, Status> {
        let mut client = self.worker_client().await?;
        let status = client.get_status(proto::Empty {}).await?.into_inner();
        Ok(Response::new(proto::WorkerList {
            workers: vec![status],
        }))
    }

    async fn get_worker_status(
        &self,
        _req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::WorkerStatus>, Status> {
        let mut client = self.worker_client().await?;
        client.get_status(proto::Empty {}).await
    }

    async fn get_worker_portfolio(
        &self,
        _req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::Portfolio>, Status> {
        let mut client = self.worker_client().await?;
        client.get_portfolio(proto::Empty {}).await
    }

    async fn update_worker_settings(
        &self,
        req: Request<proto::UpdateWorkerSettingsRequest>,
    ) -> Result<Response<proto::UpdateSettingsResponse>, Status> {
        let settings = req.into_inner().settings
            .ok_or_else(|| Status::invalid_argument("missing settings"))?;
        let mut client = self.worker_client().await?;
        client.update_settings(settings).await
    }

    async fn get_performance_report(
        &self,
        _req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::PerformanceReport>, Status> {
        Ok(Response::new(proto::PerformanceReport {
            worker_id: "test-worker".into(),
            llm_provider: "grok".into(),
            total_pnl: "550".into(),
            realized_pnl: "350".into(),
            unrealized_pnl: "200".into(),
            win_rate: "0.65".into(),
            total_trades: 20,
            winning_trades: 13,
            losing_trades: 7,
            sharpe_ratio: "1.5".into(),
            max_drawdown: "0.08".into(),
            total_predictions: 10,
            correct_predictions: 7,
            prediction_accuracy: "0.7".into(),
            period_start: Some(Timestamp { seconds: 0, nanos: 0 }),
            period_end: Some(Timestamp { seconds: 3000, nanos: 0 }),
        }))
    }

    async fn get_aggregate_performance(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::AggregatePerformanceReport>, Status> {
        let report = self
            .get_performance_report(Request::new(proto::WorkerId {
                id: "test-worker".into(),
            }))
            .await?
            .into_inner();

        Ok(Response::new(proto::AggregatePerformanceReport {
            best_worker_id: report.worker_id.clone(),
            total_fleet_pnl: report.total_pnl.clone(),
            workers: vec![report],
        }))
    }

    async fn get_worker_open_trades(
        &self,
        _req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::OpenTradesResponse>, Status> {
        let mut client = self.worker_client().await?;
        client.get_open_trades(proto::Empty {}).await
    }

    async fn get_worker_open_predictions(
        &self,
        _req: Request<proto::WorkerId>,
    ) -> Result<Response<proto::OpenPredictionsResponse>, Status> {
        let mut client = self.worker_client().await?;
        client.get_open_predictions(proto::Empty {}).await
    }

    async fn get_worker_trade_history(
        &self,
        req: Request<proto::WorkerHistoryRequest>,
    ) -> Result<Response<proto::TradeHistoryResponse>, Status> {
        let inner = req.into_inner();
        let mut client = self.worker_client().await?;
        client
            .get_trade_history(proto::HistoryRequest {
                limit: inner.limit,
                offset: inner.offset,
            })
            .await
    }

    async fn get_worker_prediction_history(
        &self,
        req: Request<proto::WorkerHistoryRequest>,
    ) -> Result<Response<proto::PredictionHistoryResponse>, Status> {
        let inner = req.into_inner();
        let mut client = self.worker_client().await?;
        client
            .get_prediction_history(proto::HistoryRequest {
                limit: inner.limit,
                offset: inner.offset,
            })
            .await
    }
}


// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn spawn_worker_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    tokio::spawn(async move {
        Server::builder()
            .add_service(WorkerServiceServer::new(MockWorker))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    // Brief yield so the server task starts accepting
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    url
}

async fn spawn_master_server(worker_url: String) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    tokio::spawn(async move {
        Server::builder()
            .add_service(MasterServiceServer::new(MockMaster {
                worker_addr: worker_url,
            }))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    url
}

fn test_settings() -> proto::BotSettings {
    proto::BotSettings {
        max_position_size_pct: "5".into(),
        stop_loss_pct: "3".into(),
        max_open_positions: 10,
        confidence_threshold: "0.7".into(),
        trading_allocation_pct: "60".into(),
        polymarket_allocation_pct: "40".into(),
        llm_provider: "grok".into(),
        llm_temperature: "0.3".into(),
        allowed_pairs: vec!["BTC/USDT".into(), "ETH/USDT".into()],
        trading_interval_secs: 300,
        max_prediction_exposure: "500".into(),
        min_liquidity_threshold: "1000".into(),
        polymarket_interval_secs: 600,
    }
}


// ---------------------------------------------------------------------------
// Worker RPC tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn worker_get_status() {
    let url = spawn_worker_server().await;
    let mut client = WorkerServiceClient::connect(url).await.unwrap();

    let resp = client.get_status(proto::Empty {}).await.unwrap().into_inner();
    assert_eq!(resp.worker_id, "test-worker");
    assert_eq!(resp.llm_provider, "grok");
    assert!(resp.trading_active);
    assert!(resp.polymarket_active);
    assert_eq!(resp.open_trade_count, 3);
    assert_eq!(resp.open_prediction_count, 2);
}

#[tokio::test]
async fn worker_get_portfolio() {
    let url = spawn_worker_server().await;
    let mut client = WorkerServiceClient::connect(url).await.unwrap();

    let resp = client.get_portfolio(proto::Empty {}).await.unwrap().into_inner();
    assert_eq!(resp.total_balance, "10000");
    assert_eq!(resp.available_balance, "8000");
    assert_eq!(resp.base_currency, "USDT");
}

#[tokio::test]
async fn worker_update_settings() {
    let url = spawn_worker_server().await;
    let mut client = WorkerServiceClient::connect(url).await.unwrap();

    let settings = test_settings();
    let resp = client.update_settings(settings.clone()).await.unwrap().into_inner();
    assert!(resp.success);
    assert_eq!(resp.message, "applied");

    let applied = resp.applied_settings.unwrap();
    assert_eq!(applied.max_open_positions, 10);
    assert_eq!(applied.allowed_pairs, vec!["BTC/USDT", "ETH/USDT"]);
}

#[tokio::test]
async fn worker_open_trades() {
    let url = spawn_worker_server().await;
    let mut client = WorkerServiceClient::connect(url).await.unwrap();

    let resp = client.get_open_trades(proto::Empty {}).await.unwrap().into_inner();
    assert_eq!(resp.positions.len(), 1);

    let pos = &resp.positions[0];
    assert_eq!(pos.pair, "BTC/USDT");
    assert_eq!(pos.side, "long");
    assert_eq!(pos.entry_price, "50000");
}

#[tokio::test]
async fn worker_open_predictions() {
    let url = spawn_worker_server().await;
    let mut client = WorkerServiceClient::connect(url).await.unwrap();

    let resp = client.get_open_predictions(proto::Empty {}).await.unwrap().into_inner();
    assert_eq!(resp.predictions.len(), 1);

    let pred = &resp.predictions[0];
    assert_eq!(pred.market_id, "mkt-abc");
    assert_eq!(pred.side, "yes");
}

#[tokio::test]
async fn worker_trade_history() {
    let url = spawn_worker_server().await;
    let mut client = WorkerServiceClient::connect(url).await.unwrap();

    let resp = client
        .get_trade_history(proto::HistoryRequest { limit: 2, offset: 0 })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.records.len(), 2);
    assert_eq!(resp.records[0].pair, "ETH/USDT");
    assert_eq!(resp.records[1].id, "trade-1");
}

#[tokio::test]
async fn worker_prediction_history() {
    let url = spawn_worker_server().await;
    let mut client = WorkerServiceClient::connect(url).await.unwrap();

    let resp = client
        .get_prediction_history(proto::HistoryRequest { limit: 1, offset: 0 })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.records.len(), 1);
    assert_eq!(resp.records[0].market_id, "mkt-xyz");
    assert_eq!(resp.records[0].action, "buy");
}


// ---------------------------------------------------------------------------
// Master RPC tests (proxied through mock worker)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn master_list_workers() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client.list_workers(proto::Empty {}).await.unwrap().into_inner();
    assert_eq!(resp.workers.len(), 1);
    assert_eq!(resp.workers[0].worker_id, "test-worker");
}

#[tokio::test]
async fn master_get_worker_status() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .get_worker_status(proto::WorkerId { id: "test-worker".into() })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.worker_id, "test-worker");
    assert!(resp.trading_active);
}

#[tokio::test]
async fn master_get_worker_portfolio() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .get_worker_portfolio(proto::WorkerId { id: "test-worker".into() })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.total_balance, "10000");
}

#[tokio::test]
async fn master_update_worker_settings() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .update_worker_settings(proto::UpdateWorkerSettingsRequest {
            worker_id: "test-worker".into(),
            settings: Some(test_settings()),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(resp.success);
    assert!(resp.applied_settings.is_some());
}

#[tokio::test]
async fn master_performance_report() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .get_performance_report(proto::WorkerId { id: "test-worker".into() })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.worker_id, "test-worker");
    assert_eq!(resp.total_pnl, "550");
    assert_eq!(resp.total_trades, 20);
    assert_eq!(resp.winning_trades, 13);
}

#[tokio::test]
async fn master_aggregate_performance() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .get_aggregate_performance(proto::Empty {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.workers.len(), 1);
    assert_eq!(resp.best_worker_id, "test-worker");
    assert_eq!(resp.total_fleet_pnl, "550");
}

#[tokio::test]
async fn master_worker_open_trades() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .get_worker_open_trades(proto::WorkerId { id: "test-worker".into() })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.positions.len(), 1);
    assert_eq!(resp.positions[0].pair, "BTC/USDT");
}

#[tokio::test]
async fn master_worker_open_predictions() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .get_worker_open_predictions(proto::WorkerId { id: "test-worker".into() })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.predictions.len(), 1);
    assert_eq!(resp.predictions[0].market_id, "mkt-abc");
}

#[tokio::test]
async fn master_worker_trade_history() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .get_worker_trade_history(proto::WorkerHistoryRequest {
            worker_id: "test-worker".into(),
            limit: 2,
            offset: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.records.len(), 2);
}

#[tokio::test]
async fn master_worker_prediction_history() {
    let worker_url = spawn_worker_server().await;
    let master_url = spawn_master_server(worker_url).await;
    let mut client = MasterServiceClient::connect(master_url).await.unwrap();

    let resp = client
        .get_worker_prediction_history(proto::WorkerHistoryRequest {
            worker_id: "test-worker".into(),
            limit: 2,
            offset: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.records.len(), 2);
    assert_eq!(resp.records[0].market_id, "mkt-xyz");
}
