use std::sync::Arc;

use chrono::Utc;
use communication::proto;
use prost_types::Timestamp;
use tonic::{Request, Response, Status};

use crate::llm::LlmProvider;
use crate::polymarket::service::PolymarketService;
use crate::portfolio::PortfolioService;
use crate::settings::SettingsService;
use crate::trading::service::TradingService;


pub struct WorkerGrpcServer {
    worker_id: String,
    started_at: chrono::DateTime<Utc>,
    trading: Arc<TradingService>,
    polymarket: Arc<PolymarketService>,
    portfolio: Arc<PortfolioService>,
    settings: Arc<SettingsService>,
}

impl WorkerGrpcServer {
    pub fn new(
        worker_id: String,
        trading: Arc<TradingService>,
        polymarket: Arc<PolymarketService>,
        portfolio: Arc<PortfolioService>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            worker_id,
            started_at: Utc::now(),
            trading,
            polymarket,
            portfolio,
            settings,
        }
    }
}


fn datetime_to_timestamp(dt: chrono::DateTime<Utc>) -> Option<Timestamp> {
    Some(Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    })
}

fn map_err(e: impl std::fmt::Display) -> Status {
    Status::internal(e.to_string())
}


#[tonic::async_trait]
impl communication::WorkerService for WorkerGrpcServer {
    async fn get_status(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::WorkerStatus>, Status> {
        let settings = self.settings.get();
        let open_trades = self
            .trading
            .get_open_positions()
            .await
            .map_err(map_err)?
            .len() as i32;
        let open_preds = self
            .polymarket
            .get_open_predictions()
            .await
            .map_err(map_err)?
            .len() as i32;

        let provider = match settings.llm_provider {
            LlmProvider::Grok => "grok",
            LlmProvider::Gemini => "gemini",
        };

        Ok(Response::new(proto::WorkerStatus {
            worker_id: self.worker_id.clone(),
            llm_provider: provider.into(),
            trading_active: true,
            polymarket_active: true,
            started_at: datetime_to_timestamp(self.started_at),
            last_heartbeat: datetime_to_timestamp(Utc::now()),
            open_trade_count: open_trades,
            open_prediction_count: open_preds,
        }))
    }

    async fn get_portfolio(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::Portfolio>, Status> {
        let state = self.portfolio.get_state().await;

        Ok(Response::new(proto::Portfolio {
            total_balance: state.total_balance.to_string(),
            available_balance: state.available_balance.to_string(),
            trading_allocated: state.trading_allocated.to_string(),
            polymarket_allocated: state.polymarket_allocated.to_string(),
            unrealized_pnl: state.unrealized_pnl.to_string(),
            realized_pnl: state.realized_pnl.to_string(),
            base_currency: state.base_currency,
        }))
    }

    async fn update_settings(
        &self,
        req: Request<proto::BotSettings>,
    ) -> Result<Response<proto::UpdateSettingsResponse>, Status> {
        let proto_settings = req.into_inner();
        let new_settings = parse_bot_settings(&proto_settings)?;

        let applied = self
            .settings
            .replace(new_settings)
            .await
            .map_err(map_err)?;

        // Re-evaluate open positions/predictions against new constraints
        if let Err(e) = self.trading.reevaluate_positions(&applied).await {
            tracing::error!(error = %e, "Failed to reevaluate trading positions");
        }
        if let Err(e) = self.polymarket.reevaluate_predictions(&applied).await {
            tracing::error!(error = %e, "Failed to reevaluate polymarket predictions");
        }

        Ok(Response::new(proto::UpdateSettingsResponse {
            success: true,
            message: "Settings applied and positions reevaluated".into(),
            applied_settings: Some(settings_to_proto(&applied)),
        }))
    }

    async fn get_open_trades(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::OpenTradesResponse>, Status> {
        let positions = self
            .trading
            .get_open_positions()
            .await
            .map_err(map_err)?;

        let proto_positions = positions.into_iter().map(position_to_proto).collect();
        Ok(Response::new(proto::OpenTradesResponse {
            positions: proto_positions,
        }))
    }

    async fn get_open_predictions(
        &self,
        _req: Request<proto::Empty>,
    ) -> Result<Response<proto::OpenPredictionsResponse>, Status> {
        let predictions = self
            .polymarket
            .get_open_predictions()
            .await
            .map_err(map_err)?;

        let proto_preds = predictions.into_iter().map(prediction_to_proto).collect();
        Ok(Response::new(proto::OpenPredictionsResponse {
            predictions: proto_preds,
        }))
    }

    async fn get_trade_history(
        &self,
        req: Request<proto::HistoryRequest>,
    ) -> Result<Response<proto::TradeHistoryResponse>, Status> {
        let params = req.into_inner();
        let page_size = if params.limit > 0 {
            params.limit as u64
        } else {
            50
        };

        let records = self
            .trading
            .get_trade_history(page_size)
            .await
            .map_err(map_err)?;

        let proto_records = records.into_iter().map(trade_record_to_proto).collect();
        Ok(Response::new(proto::TradeHistoryResponse {
            records: proto_records,
        }))
    }

    async fn get_prediction_history(
        &self,
        req: Request<proto::HistoryRequest>,
    ) -> Result<Response<proto::PredictionHistoryResponse>, Status> {
        let params = req.into_inner();
        let page_size = if params.limit > 0 {
            params.limit as u64
        } else {
            50
        };

        let records = self
            .polymarket
            .get_prediction_history(page_size)
            .await
            .map_err(map_err)?;

        let proto_records = records
            .into_iter()
            .map(prediction_record_to_proto)
            .collect();
        Ok(Response::new(proto::PredictionHistoryResponse {
            records: proto_records,
        }))
    }
}


// ---------------------------------------------------------------------------
// Proto <-> Domain conversions
// ---------------------------------------------------------------------------

fn parse_decimal(s: &str, field: &str) -> Result<rust_decimal::Decimal, Status> {
    s.parse()
        .map_err(|_| Status::invalid_argument(format!("Invalid decimal in {field}: {s}")))
}

fn parse_bot_settings(p: &proto::BotSettings) -> Result<crate::settings::BotSettings, Status> {
    let provider = match p.llm_provider.to_lowercase().as_str() {
        "grok" => LlmProvider::Grok,
        "gemini" => LlmProvider::Gemini,
        other => return Err(Status::invalid_argument(format!("Unknown llm_provider: {other}"))),
    };

    Ok(crate::settings::BotSettings {
        max_position_size_pct: parse_decimal(&p.max_position_size_pct, "max_position_size_pct")?,
        stop_loss_pct: parse_decimal(&p.stop_loss_pct, "stop_loss_pct")?,
        max_open_positions: p.max_open_positions,
        confidence_threshold: parse_decimal(&p.confidence_threshold, "confidence_threshold")?,
        trading_allocation_pct: parse_decimal(
            &p.trading_allocation_pct,
            "trading_allocation_pct",
        )?,
        polymarket_allocation_pct: parse_decimal(
            &p.polymarket_allocation_pct,
            "polymarket_allocation_pct",
        )?,
        llm_provider: provider,
        llm_temperature: parse_decimal(&p.llm_temperature, "llm_temperature")?,
        allowed_pairs: p.allowed_pairs.clone(),
        trading_interval_secs: p.trading_interval_secs,
        max_prediction_exposure: parse_decimal(
            &p.max_prediction_exposure,
            "max_prediction_exposure",
        )?,
        min_liquidity_threshold: parse_decimal(
            &p.min_liquidity_threshold,
            "min_liquidity_threshold",
        )?,
        polymarket_interval_secs: p.polymarket_interval_secs,
    })
}

fn settings_to_proto(s: &crate::settings::BotSettings) -> proto::BotSettings {
    let provider = match s.llm_provider {
        LlmProvider::Grok => "grok",
        LlmProvider::Gemini => "gemini",
    };

    proto::BotSettings {
        max_position_size_pct: s.max_position_size_pct.to_string(),
        stop_loss_pct: s.stop_loss_pct.to_string(),
        max_open_positions: s.max_open_positions,
        confidence_threshold: s.confidence_threshold.to_string(),
        trading_allocation_pct: s.trading_allocation_pct.to_string(),
        polymarket_allocation_pct: s.polymarket_allocation_pct.to_string(),
        llm_provider: provider.into(),
        llm_temperature: s.llm_temperature.to_string(),
        allowed_pairs: s.allowed_pairs.clone(),
        trading_interval_secs: s.trading_interval_secs,
        max_prediction_exposure: s.max_prediction_exposure.to_string(),
        min_liquidity_threshold: s.min_liquidity_threshold.to_string(),
        polymarket_interval_secs: s.polymarket_interval_secs,
    }
}

fn position_to_proto(
    p: crate::trading::entities::open_position::Model,
) -> proto::OpenPosition {
    proto::OpenPosition {
        id: p.id.to_string(),
        pair: p.pair,
        side: p.side,
        entry_price: p.entry_price.to_string(),
        quantity: p.quantity.to_string(),
        current_price: p.current_price.to_string(),
        unrealized_pnl: p.unrealized_pnl.to_string(),
        stop_loss_price: p.stop_loss_price.map(|d| d.to_string()).unwrap_or_default(),
        take_profit_price: p.take_profit_price.map(|d| d.to_string()).unwrap_or_default(),
        allocated_capital: p.allocated_capital.to_string(),
        opened_at: datetime_to_timestamp(p.opened_at),
        updated_at: datetime_to_timestamp(p.updated_at),
    }
}

fn trade_record_to_proto(
    r: crate::trading::entities::trade_record::Model,
) -> proto::TradeRecord {
    proto::TradeRecord {
        id: r.id.to_string(),
        pair: r.pair,
        side: r.side,
        order_type: r.order_type,
        quantity: r.quantity.to_string(),
        price: r.price.to_string(),
        filled_quantity: r.filled_quantity.to_string(),
        avg_fill_price: r.avg_fill_price.to_string(),
        fee: r.fee.to_string(),
        status: r.status,
        llm_rationale: r.llm_rationale.unwrap_or_default(),
        llm_confidence: r.llm_confidence.map(|d| d.to_string()).unwrap_or_default(),
        created_at: datetime_to_timestamp(r.created_at),
        updated_at: datetime_to_timestamp(r.updated_at),
    }
}

fn prediction_to_proto(
    p: crate::polymarket::entities::open_prediction::Model,
) -> proto::OpenPrediction {
    proto::OpenPrediction {
        id: p.id.to_string(),
        market_id: p.market_id,
        market_title: p.market_title,
        side: p.side,
        shares: p.shares.to_string(),
        avg_price: p.avg_price.to_string(),
        current_price: p.current_price.to_string(),
        unrealized_pnl: p.unrealized_pnl.to_string(),
        allocated_capital: p.allocated_capital.to_string(),
        opened_at: datetime_to_timestamp(p.opened_at),
        updated_at: datetime_to_timestamp(p.updated_at),
    }
}

fn prediction_record_to_proto(
    r: crate::polymarket::entities::prediction_record::Model,
) -> proto::PredictionRecord {
    proto::PredictionRecord {
        id: r.id.to_string(),
        market_id: r.market_id,
        market_title: r.market_title,
        side: r.side,
        action: r.action,
        shares: r.shares.to_string(),
        price_per_share: r.price_per_share.to_string(),
        total_cost: r.total_cost.to_string(),
        status: r.status,
        resolution: r.resolution.unwrap_or_default(),
        llm_rationale: r.llm_rationale.unwrap_or_default(),
        llm_confidence: r.llm_confidence.map(|d| d.to_string()).unwrap_or_default(),
        created_at: datetime_to_timestamp(r.created_at),
        updated_at: datetime_to_timestamp(r.updated_at),
    }
}
