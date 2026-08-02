use sea_orm::entity::prelude::*;

/// Persisted to DB for recovery and historical tracking.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "portfolio_snapshots")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub total_balance: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub available_balance: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub trading_allocated: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub polymarket_allocated: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub unrealized_pnl: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub realized_pnl: Decimal,
    pub base_currency: String,
    pub snapshot_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
