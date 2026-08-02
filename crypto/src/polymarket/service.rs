use std::sync::Arc;

use chrono::Utc;
use rust_decimal::Decimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use crate::error::{CryptoError, Result};
use crate::events::{BotEvent, EventBus};
use crate::portfolio::PortfolioService;
use crate::settings::SettingsService;

use super::entities::{open_prediction, prediction_record};
use super::executor::PolymarketExecutor;
use super::models::*;
use super::strategy::PolymarketStrategy;

pub struct PolymarketService {
    db: DatabaseConnection,
    strategy: Box<dyn PolymarketStrategy>,
    executor: Box<dyn PolymarketExecutor>,
    portfolio: Arc<PortfolioService>,
    settings: Arc<SettingsService>,
    event_bus: Arc<EventBus>,
}

impl PolymarketService {
    pub fn new(
        db: DatabaseConnection,
        strategy: Box<dyn PolymarketStrategy>,
        executor: Box<dyn PolymarketExecutor>,
        portfolio: Arc<PortfolioService>,
        settings: Arc<SettingsService>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            db,
            strategy,
            executor,
            portfolio,
            settings,
            event_bus,
        }
    }

    pub async fn tick(&self) -> Result<()> {
        let settings = self.settings.get();
        let markets = self.executor.fetch_active_markets().await?;

        for market in &markets {
            if let Err(e) = self.process_market(market, &settings).await {
                tracing::error!(
                    market_id = %market.market_id,
                    error = %e,
                    "Failed to process market"
                );
                self.event_bus.publish(BotEvent::ModuleError {
                    module: "polymarket".into(),
                    error: format!("Market {} error: {e}", market.market_id),
                });
            }
        }

        self.update_prediction_prices().await?;
        Ok(())
    }

    pub async fn update_prediction_prices(&self) -> Result<()> {
        let preds = open_prediction::Entity::find().all(&self.db).await?;

        for pred in preds {
            let ob = self.executor.get_market_prices(&pred.market_id).await?;
            let current = match pred.side.as_str() {
                "yes" => ob.yes_price,
                "no" => ob.no_price,
                _ => continue,
            };
            let pnl = (current - pred.avg_price) * pred.shares;

            let mut active: open_prediction::ActiveModel = pred.into();
            active.current_price = Set(current);
            active.unrealized_pnl = Set(pnl);
            active.updated_at = Set(Utc::now());
            active.update(&self.db).await?;
        }
        Ok(())
    }

    pub async fn get_open_predictions(
        &self,
    ) -> Result<Vec<open_prediction::Model>> {
        Ok(open_prediction::Entity::find()
            .order_by_asc(open_prediction::Column::OpenedAt)
            .all(&self.db)
            .await?)
    }

    pub async fn get_prediction_history(
        &self,
        page_size: u64,
    ) -> Result<Vec<prediction_record::Model>> {
        Ok(prediction_record::Entity::find()
            .order_by_desc(prediction_record::Column::CreatedAt)
            .paginate(&self.db, page_size)
            .fetch_page(0)
            .await?)
    }

    /// Re-check all open predictions against updated settings. Called after a
    /// settings push from the master.
    ///
    /// If total exposure exceeds the new `max_prediction_exposure`, sell
    /// predictions starting from the smallest position until compliant.
    pub async fn reevaluate_predictions(
        &self,
        settings: &crate::settings::BotSettings,
    ) -> Result<()> {
        let mut predictions = open_prediction::Entity::find()
            .order_by_desc(open_prediction::Column::AllocatedCapital)
            .all(&self.db)
            .await?;

        let mut total_exposure: Decimal =
            predictions.iter().map(|p| p.allocated_capital).sum();

        // Sell smallest positions first until within limits
        predictions.reverse();
        while total_exposure > settings.max_prediction_exposure {
            let Some(pred) = predictions.first() else {
                break;
            };

            tracing::warn!(
                market_id = %pred.market_id,
                capital = %pred.allocated_capital,
                "Closing prediction: total exposure {} exceeds limit {}",
                total_exposure,
                settings.max_prediction_exposure,
            );

            self.force_close_prediction(pred).await?;
            total_exposure -= pred.allocated_capital;
            predictions.remove(0);
        }

        Ok(())
    }

    /// Market-sell a prediction and release its capital.
    async fn force_close_prediction(
        &self,
        pred: &open_prediction::Model,
    ) -> Result<()> {
        let side = match pred.side.as_str() {
            "yes" => PredictionSide::Yes,
            _ => PredictionSide::No,
        };

        let signal = PredictionSignal {
            market_id: pred.market_id.clone(),
            market_title: pred.market_title.clone(),
            side,
            action: PredictionOrderAction::Sell,
            shares: pred.shares,
            limit_price: None,
            confidence: Decimal::ONE,
            rationale: "Settings reevaluation: force close".into(),
            timestamp: Utc::now(),
        };

        match self.executor.execute(&signal).await {
            Ok(result) => {
                let pnl =
                    (result.avg_price - pred.avg_price) * result.filled_shares;
                self.portfolio
                    .release_funds(pred.allocated_capital, pnl, "polymarket")
                    .await?;
                open_prediction::Entity::delete_by_id(pred.id)
                    .exec(&self.db)
                    .await?;
                self.record_prediction(&signal, &result).await?;
            }
            Err(e) => {
                tracing::error!(
                    market_id = %pred.market_id,
                    error = %e,
                    "Force-close prediction failed"
                );
                self.event_bus.publish(BotEvent::ModuleError {
                    module: "polymarket".into(),
                    error: format!(
                        "Force-close failed for {}: {e}",
                        pred.market_id
                    ),
                });
            }
        }

        Ok(())
    }

    async fn process_market(
        &self,
        market: &MarketInfo,
        settings: &crate::settings::BotSettings,
    ) -> Result<()> {
        let ob = self.executor.get_market_prices(&market.market_id).await?;
        let Some(signal) = self.strategy.analyze(market, &ob, settings).await?
        else {
            return Ok(());
        };

        self.check_exposure(&signal, settings).await?;

        let cost =
            signal.shares * signal.limit_price.unwrap_or(Decimal::new(5, 1));
        if signal.action == PredictionOrderAction::Buy {
            self.portfolio.reserve_funds(cost, "polymarket").await?;
        }

        let result = match self.executor.execute(&signal).await {
            Ok(r) => r,
            Err(e) => {
                if signal.action == PredictionOrderAction::Buy {
                    self.portfolio
                        .release_funds(cost, Decimal::ZERO, "polymarket")
                        .await?;
                }
                return Err(e);
            }
        };

        self.record_prediction(&signal, &result).await?;

        if result.status == PredictionStatus::Filled {
            self.update_open_predictions(&signal, &result).await?;
        }

        self.event_bus.publish(BotEvent::PredictionPlaced {
            prediction_id: Uuid::new_v4(),
            market_id: result.market_id.clone(),
            side: result.side.to_string(),
            shares: result.filled_shares,
        });

        Ok(())
    }

    async fn check_exposure(
        &self,
        signal: &PredictionSignal,
        settings: &crate::settings::BotSettings,
    ) -> Result<()> {
        let open = open_prediction::Entity::find().all(&self.db).await?;

        let total_exposure: Decimal =
            open.iter().map(|p| p.allocated_capital).sum();
        let signal_cost =
            signal.shares * signal.limit_price.unwrap_or(Decimal::ZERO);

        if total_exposure + signal_cost > settings.max_prediction_exposure {
            return Err(CryptoError::RiskLimitExceeded(format!(
                "Polymarket exposure would exceed limit: {} + {} > {}",
                total_exposure, signal_cost, settings.max_prediction_exposure
            )));
        }

        // Per-market cap = total limit / 3
        let market_exposure: Decimal = open
            .iter()
            .filter(|p| p.market_id == signal.market_id)
            .map(|p| p.allocated_capital)
            .sum();
        let per_market_limit =
            settings.max_prediction_exposure / Decimal::new(3, 0);

        if market_exposure + signal_cost > per_market_limit {
            return Err(CryptoError::RiskLimitExceeded(format!(
                "Per-market exposure exceeded for {}",
                signal.market_id
            )));
        }

        Ok(())
    }

    async fn record_prediction(
        &self,
        signal: &PredictionSignal,
        result: &PredictionResult,
    ) -> Result<()> {
        let now = Utc::now();
        prediction_record::ActiveModel {
            id: Set(Uuid::new_v4()),
            market_id: Set(signal.market_id.clone()),
            market_title: Set(signal.market_title.clone()),
            side: Set(signal.side.to_string()),
            action: Set(signal.action.to_string()),
            shares: Set(result.filled_shares),
            price_per_share: Set(result.avg_price),
            total_cost: Set(result.total_cost),
            status: Set(result.status.to_string()),
            resolution: Set(None),
            llm_rationale: Set(Some(signal.rationale.clone())),
            llm_confidence: Set(Some(signal.confidence)),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    async fn update_open_predictions(
        &self,
        signal: &PredictionSignal,
        result: &PredictionResult,
    ) -> Result<()> {
        let now = Utc::now();

        match signal.action {
            PredictionOrderAction::Buy => {
                let existing = open_prediction::Entity::find()
                    .filter(
                        open_prediction::Column::MarketId.eq(&signal.market_id),
                    )
                    .filter(
                        open_prediction::Column::Side
                            .eq(signal.side.to_string()),
                    )
                    .one(&self.db)
                    .await?;

                if let Some(pos) = existing {
                    let total_shares = pos.shares + result.filled_shares;
                    let total_cost = (pos.avg_price * pos.shares)
                        + (result.avg_price * result.filled_shares);
                    let new_avg = if total_shares > Decimal::ZERO {
                        total_cost / total_shares
                    } else {
                        Decimal::ZERO
                    };

                    let mut active: open_prediction::ActiveModel = pos.into();
                    active.shares = Set(total_shares);
                    active.avg_price = Set(new_avg);
                    active.allocated_capital = Set(total_cost);
                    active.updated_at = Set(now);
                    active.update(&self.db).await?;
                } else {
                    open_prediction::ActiveModel {
                        id: Set(Uuid::new_v4()),
                        market_id: Set(signal.market_id.clone()),
                        market_title: Set(signal.market_title.clone()),
                        side: Set(signal.side.to_string()),
                        shares: Set(result.filled_shares),
                        avg_price: Set(result.avg_price),
                        current_price: Set(result.avg_price),
                        unrealized_pnl: Set(Decimal::ZERO),
                        allocated_capital: Set(result.total_cost),
                        opened_at: Set(now),
                        updated_at: Set(now),
                    }
                    .insert(&self.db)
                    .await?;
                }
            }
            PredictionOrderAction::Sell => {
                if let Some(pos) = open_prediction::Entity::find()
                    .filter(
                        open_prediction::Column::MarketId.eq(&signal.market_id),
                    )
                    .filter(
                        open_prediction::Column::Side
                            .eq(signal.side.to_string()),
                    )
                    .one(&self.db)
                    .await?
                {
                    let pnl = (result.avg_price - pos.avg_price)
                        * result.filled_shares;
                    self.portfolio
                        .release_funds(pos.allocated_capital, pnl, "polymarket")
                        .await?;
                    open_prediction::Entity::delete_by_id(pos.id)
                        .exec(&self.db)
                        .await?;
                }
            }
        }
        Ok(())
    }
}
