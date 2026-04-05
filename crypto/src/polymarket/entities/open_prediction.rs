use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "open_predictions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub market_id: String,
    pub market_title: String,
    /// "yes" | "no"
    pub side: String,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub shares: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub avg_price: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub current_price: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub unrealized_pnl: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub allocated_capital: Decimal,
    pub opened_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
