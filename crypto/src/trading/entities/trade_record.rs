use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "trade_records")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub pair: String,
    /// "buy" | "sell"
    pub side: String,
    /// "market" | "limit"
    pub order_type: String,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub quantity: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub price: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub filled_quantity: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub avg_fill_price: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub fee: Decimal,
    /// "pending" | "filled" | "partial" | "cancelled" | "failed"
    pub status: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub llm_rationale: Option<String>,
    #[sea_orm(column_type = "Decimal(Some((5, 4)))", nullable)]
    pub llm_confidence: Option<Decimal>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
