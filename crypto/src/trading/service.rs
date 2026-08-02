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

use super::entities::{open_position, trade_record};
use super::executor::TradeExecutor;
use super::models::*;
use super::strategy::TradingStrategy;

pub struct TradingService {
    db: DatabaseConnection,
    strategy: Box<dyn TradingStrategy>,
    executor: Box<dyn TradeExecutor>,
    portfolio: Arc<PortfolioService>,
    settings: Arc<SettingsService>,
    event_bus: Arc<EventBus>,
}

impl TradingService {
    pub fn new(
        db: DatabaseConnection,
        strategy: Box<dyn TradingStrategy>,
        executor: Box<dyn TradeExecutor>,
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

    pub async fn tick(
        &self,
        market_data: &MarketData,
        indicators: &TechnicalIndicators,
    ) -> Result<()> {
        let settings = self.settings.get();

        if !settings.allowed_pairs.contains(&market_data.pair) {
            return Ok(());
        }

        let Some(mut signal) = self
            .strategy
            .analyze(market_data, indicators, &settings)
            .await?
        else {
            return Ok(());
        };

        let balance = self.portfolio.get_balance().await;
        let trading_budget =
            balance * settings.trading_allocation_pct / Decimal::new(100, 0);
        let position_cap = trading_budget * settings.max_position_size_pct
            / Decimal::new(100, 0);

        if market_data.price > Decimal::ZERO {
            signal.quantity = position_cap / market_data.price;
        }

        signal.stop_loss = Some(
            market_data.price
                * (Decimal::ONE
                    - settings.stop_loss_pct / Decimal::new(100, 0)),
        );

        self.check_risk(&signal, &settings).await?;

        let capital = signal.quantity * market_data.price;
        self.portfolio.reserve_funds(capital, "trading").await?;

        let result = match self.executor.execute(&signal).await {
            Ok(r) => r,
            Err(e) => {
                self.portfolio
                    .release_funds(capital, Decimal::ZERO, "trading")
                    .await?;
                return Err(e);
            }
        };

        self.record_trade(&signal, &result).await?;

        if result.status == TradeStatus::Filled
            || result.status == TradeStatus::PartiallyFilled
        {
            self.update_positions(&signal, &result).await?;
        }

        self.event_bus.publish(BotEvent::TradeExecuted {
            trade_id: Uuid::new_v4(),
            pair: result.pair.clone(),
            side: result.side.to_string(),
            quantity: result.filled_quantity,
            price: result.avg_fill_price,
        });

        Ok(())
    }

    pub async fn update_position_prices(&self) -> Result<()> {
        let positions = open_position::Entity::find().all(&self.db).await?;
        let mut total_unrealized = Decimal::ZERO;

        for pos in positions {
            let current_price = self.executor.get_price(&pos.pair).await?;
            let pnl = match pos.side.as_str() {
                "long" => (current_price - pos.entry_price) * pos.quantity,
                "short" => (pos.entry_price - current_price) * pos.quantity,
                _ => Decimal::ZERO,
            };

            let mut active: open_position::ActiveModel = pos.into();
            active.current_price = Set(current_price);
            active.unrealized_pnl = Set(pnl);
            active.updated_at = Set(Utc::now());
            active.update(&self.db).await?;

            total_unrealized += pnl;
        }

        self.portfolio.update_unrealized_pnl(total_unrealized).await;
        Ok(())
    }

    pub async fn check_stop_losses(&self) -> Result<()> {
        let positions = open_position::Entity::find().all(&self.db).await?;

        for pos in positions {
            let Some(stop) = pos.stop_loss_price else {
                continue;
            };

            let triggered = match pos.side.as_str() {
                "long" => pos.current_price <= stop,
                "short" => pos.current_price >= stop,
                _ => false,
            };

            if !triggered {
                continue;
            }

            tracing::warn!(
                pair = %pos.pair,
                current = %pos.current_price,
                stop = %stop,
                "Stop-loss triggered"
            );

            let signal = TradeSignal {
                pair: pos.pair.clone(),
                side: TradeSide::Sell,
                order_type: OrderType::Market,
                quantity: pos.quantity,
                price: None,
                stop_loss: None,
                take_profit: None,
                confidence: Decimal::ONE,
                rationale: "Stop-loss triggered".into(),
                timestamp: Utc::now(),
            };

            match self.executor.execute(&signal).await {
                Ok(result) => {
                    let pnl = (result.avg_fill_price - pos.entry_price)
                        * pos.quantity
                        - result.fee;
                    self.portfolio
                        .release_funds(pos.allocated_capital, pnl, "trading")
                        .await?;
                    open_position::Entity::delete_by_id(pos.id)
                        .exec(&self.db)
                        .await?;
                    self.record_trade(&signal, &result).await?;
                }
                Err(e) => {
                    tracing::error!(error = %e, "Stop-loss execution failed");
                    self.event_bus.publish(BotEvent::ModuleError {
                        module: "trading".into(),
                        error: format!("Stop-loss execution failed: {e}"),
                    });
                }
            }
        }

        Ok(())
    }

    pub async fn get_open_positions(
        &self,
    ) -> Result<Vec<open_position::Model>> {
        Ok(open_position::Entity::find()
            .order_by_asc(open_position::Column::OpenedAt)
            .all(&self.db)
            .await?)
    }

    pub async fn get_trade_history(
        &self,
        page_size: u64,
    ) -> Result<Vec<trade_record::Model>> {
        Ok(trade_record::Entity::find()
            .order_by_desc(trade_record::Column::CreatedAt)
            .paginate(&self.db, page_size)
            .fetch_page(0)
            .await?)
    }

    /// Re-check all open positions against updated settings. Called after a
    /// settings push from the master.
    ///
    /// 1. Close positions whose pair was removed from `allowed_pairs`.
    /// 2. Recalculate stop-loss prices using the new `stop_loss_pct`.
    /// 3. If position count exceeds the new `max_open_positions`, close the
    ///    newest positions first (LIFO) until compliant.
    pub async fn reevaluate_positions(
        &self,
        settings: &crate::settings::BotSettings,
    ) -> Result<()> {
        let mut positions = open_position::Entity::find()
            .order_by_asc(open_position::Column::OpenedAt)
            .all(&self.db)
            .await?;

        // 1. Close positions for removed pairs
        let mut kept = Vec::new();
        for pos in positions.drain(..) {
            if !settings.allowed_pairs.contains(&pos.pair) {
                tracing::warn!(pair = %pos.pair, "Closing position: pair removed from allowed list");
                self.force_close_position(&pos).await?;
            } else {
                kept.push(pos);
            }
        }

        // 2. Recalculate stop-losses on remaining positions
        let stop_factor =
            Decimal::ONE - settings.stop_loss_pct / Decimal::new(100, 0);
        for pos in &kept {
            let new_stop = pos.entry_price * stop_factor;
            let mut active: open_position::ActiveModel = pos.clone().into();
            active.stop_loss_price = Set(Some(new_stop));
            active.updated_at = Set(Utc::now());
            active.update(&self.db).await?;
        }

        // 3. Close excess positions (LIFO: newest first)
        let max = settings.max_open_positions as usize;
        if kept.len() > max {
            let excess = &kept[max..];
            for pos in excess.iter().rev() {
                tracing::warn!(
                    pair = %pos.pair,
                    "Closing position: max_open_positions reduced to {}",
                    max
                );
                self.force_close_position(pos).await?;
            }
        }

        Ok(())
    }

    /// Market-sell a position and release its capital.
    async fn force_close_position(
        &self,
        pos: &open_position::Model,
    ) -> Result<()> {
        let signal = TradeSignal {
            pair: pos.pair.clone(),
            side: TradeSide::Sell,
            order_type: OrderType::Market,
            quantity: pos.quantity,
            price: None,
            stop_loss: None,
            take_profit: None,
            confidence: Decimal::ONE,
            rationale: "Settings reevaluation: force close".into(),
            timestamp: Utc::now(),
        };

        match self.executor.execute(&signal).await {
            Ok(result) => {
                let pnl = match pos.side.as_str() {
                    "long" => {
                        (result.avg_fill_price - pos.entry_price) * pos.quantity
                            - result.fee
                    }
                    "short" => {
                        (pos.entry_price - result.avg_fill_price) * pos.quantity
                            - result.fee
                    }
                    _ => Decimal::ZERO,
                };
                self.portfolio
                    .release_funds(pos.allocated_capital, pnl, "trading")
                    .await?;
                open_position::Entity::delete_by_id(pos.id)
                    .exec(&self.db)
                    .await?;
                self.record_trade(&signal, &result).await?;
            }
            Err(e) => {
                tracing::error!(pair = %pos.pair, error = %e, "Force-close execution failed");
                self.event_bus.publish(BotEvent::ModuleError {
                    module: "trading".into(),
                    error: format!("Force-close failed for {}: {e}", pos.pair),
                });
            }
        }

        Ok(())
    }

    async fn check_risk(
        &self,
        signal: &TradeSignal,
        settings: &crate::settings::BotSettings,
    ) -> Result<()> {
        let open_count = open_position::Entity::find().count(&self.db).await?;

        if open_count >= settings.max_open_positions as u64 {
            return Err(CryptoError::RiskLimitExceeded(format!(
                "Max open positions reached: {open_count}/{}",
                settings.max_open_positions
            )));
        }

        // Prevent duplicate long positions in the same pair
        if signal.side == TradeSide::Buy {
            let exists = open_position::Entity::find()
                .filter(open_position::Column::Pair.eq(&signal.pair))
                .one(&self.db)
                .await?;
            if exists.is_some() {
                return Err(CryptoError::RiskLimitExceeded(format!(
                    "Already have an open position in {}",
                    signal.pair
                )));
            }
        }

        Ok(())
    }

    async fn record_trade(
        &self,
        signal: &TradeSignal,
        result: &TradeResult,
    ) -> Result<()> {
        let now = Utc::now();
        trade_record::ActiveModel {
            id: Set(Uuid::new_v4()),
            pair: Set(signal.pair.clone()),
            side: Set(signal.side.to_string()),
            order_type: Set(signal.order_type.to_string()),
            quantity: Set(signal.quantity),
            price: Set(signal.price.unwrap_or(Decimal::ZERO)),
            filled_quantity: Set(result.filled_quantity),
            avg_fill_price: Set(result.avg_fill_price),
            fee: Set(result.fee),
            status: Set(result.status.to_string()),
            llm_rationale: Set(Some(signal.rationale.clone())),
            llm_confidence: Set(Some(signal.confidence)),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    async fn update_positions(
        &self,
        signal: &TradeSignal,
        result: &TradeResult,
    ) -> Result<()> {
        let now = Utc::now();

        match signal.side {
            TradeSide::Buy => {
                open_position::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    pair: Set(signal.pair.clone()),
                    side: Set(PositionSide::Long.to_string()),
                    entry_price: Set(result.avg_fill_price),
                    quantity: Set(result.filled_quantity),
                    current_price: Set(result.avg_fill_price),
                    unrealized_pnl: Set(Decimal::ZERO),
                    stop_loss_price: Set(signal.stop_loss),
                    take_profit_price: Set(signal.take_profit),
                    allocated_capital: Set(
                        result.filled_quantity * result.avg_fill_price
                    ),
                    opened_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(&self.db)
                .await?;
            }
            TradeSide::Sell => {
                if let Some(pos) = open_position::Entity::find()
                    .filter(open_position::Column::Pair.eq(&signal.pair))
                    .one(&self.db)
                    .await?
                {
                    let pnl = (result.avg_fill_price - pos.entry_price)
                        * pos.quantity
                        - result.fee;
                    self.portfolio
                        .release_funds(pos.allocated_capital, pnl, "trading")
                        .await?;
                    open_position::Entity::delete_by_id(pos.id)
                        .exec(&self.db)
                        .await?;
                }
            }
        }

        Ok(())
    }
}
