use chrono::Utc;
use rust_decimal::Decimal;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, QueryOrder, Set};
use uuid::Uuid;

use crate::entities::portfolio_snapshot;
use crate::error::{CryptoError, Result};


#[derive(Debug, Clone)]
pub struct PortfolioState {
    pub total_balance: Decimal,
    pub available_balance: Decimal,
    pub trading_allocated: Decimal,
    pub polymarket_allocated: Decimal,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
    pub base_currency: String,
}

impl Default for PortfolioState {
    fn default() -> Self {
        Self {
            total_balance: Decimal::ZERO,
            available_balance: Decimal::ZERO,
            trading_allocated: Decimal::ZERO,
            polymarket_allocated: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            realized_pnl: Decimal::ZERO,
            base_currency: "USDC".into(),
        }
    }
}


/// Manages the portfolio state with RwLock-protected concurrent access.
/// Both modules call [`reserve_funds`] before execution and
/// [`release_funds`] when closing positions.  Snapshots are persisted
/// periodically and on graceful shutdown.
pub struct PortfolioService {
    db: DatabaseConnection,
    state: tokio::sync::RwLock<PortfolioState>,
}

impl PortfolioService {
    pub fn new(db: DatabaseConnection, initial_state: PortfolioState) -> Self {
        Self {
            db,
            state: tokio::sync::RwLock::new(initial_state),
        }
    }

    /// Restore the most recent snapshot from the database (if any).
    pub async fn load_latest(&self) -> Result<Option<PortfolioState>> {
        let snapshot = portfolio_snapshot::Entity::find()
            .order_by_desc(portfolio_snapshot::Column::SnapshotAt)
            .one(&self.db)
            .await?;

        if let Some(s) = &snapshot {
            let restored = PortfolioState {
                total_balance: s.total_balance,
                available_balance: s.available_balance,
                trading_allocated: s.trading_allocated,
                polymarket_allocated: s.polymarket_allocated,
                unrealized_pnl: s.unrealized_pnl,
                realized_pnl: s.realized_pnl,
                base_currency: s.base_currency.clone(),
            };
            *self.state.write().await = restored.clone();
            return Ok(Some(restored));
        }

        Ok(None)
    }

    pub async fn get_balance(&self) -> Decimal {
        self.state.read().await.available_balance
    }

    pub async fn get_state(&self) -> PortfolioState {
        self.state.read().await.clone()
    }

    /// Reserve `amount` for `module` ("trading" | "polymarket").
    pub async fn reserve_funds(&self, amount: Decimal, module: &str) -> Result<()> {
        let mut state = self.state.write().await;
        if state.available_balance < amount {
            return Err(CryptoError::InsufficientBalance {
                available: state.available_balance,
                required: amount,
            });
        }
        state.available_balance -= amount;
        match module {
            "trading" => state.trading_allocated += amount,
            "polymarket" => state.polymarket_allocated += amount,
            _ => {
                return Err(CryptoError::Portfolio(format!(
                    "Unknown module: {module}"
                )))
            }
        }
        Ok(())
    }

    /// Release capital back and book realised P&L.
    pub async fn release_funds(
        &self,
        original: Decimal,
        pnl: Decimal,
        module: &str,
    ) -> Result<()> {
        let mut state = self.state.write().await;
        let returned = original + pnl;
        state.available_balance += returned;
        match module {
            "trading" => state.trading_allocated -= original,
            "polymarket" => state.polymarket_allocated -= original,
            _ => {
                return Err(CryptoError::Portfolio(format!(
                    "Unknown module: {module}"
                )))
            }
        }
        state.realized_pnl += pnl;
        state.total_balance =
            state.available_balance + state.trading_allocated + state.polymarket_allocated;
        Ok(())
    }

    pub async fn update_unrealized_pnl(&self, unrealized: Decimal) {
        self.state.write().await.unrealized_pnl = unrealized;
    }

    /// Persist a snapshot row.
    pub async fn persist_snapshot(&self) -> Result<()> {
        let state = self.state.read().await;
        let now = Utc::now();
        let snapshot = portfolio_snapshot::ActiveModel {
            id: Set(Uuid::new_v4()),
            total_balance: Set(state.total_balance),
            available_balance: Set(state.available_balance),
            trading_allocated: Set(state.trading_allocated),
            polymarket_allocated: Set(state.polymarket_allocated),
            unrealized_pnl: Set(state.unrealized_pnl),
            realized_pnl: Set(state.realized_pnl),
            base_currency: Set(state.base_currency.clone()),
            snapshot_at: Set(now),
            created_at: Set(now),
        };
        snapshot.insert(&self.db).await?;
        Ok(())
    }

    /// Return (realized, unrealized) P&L.
    pub async fn calculate_pnl(&self) -> (Decimal, Decimal) {
        let state = self.state.read().await;
        (state.realized_pnl, state.unrealized_pnl)
    }
}
