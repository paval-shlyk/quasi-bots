use sea_orm::entity::prelude::*;

/// Historical record of every prediction order (buy/sell of Yes/No shares).
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "prediction_records")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub market_id: String,
    pub market_title: String,
    /// "yes" | "no"
    pub side: String,
    /// "buy" | "sell"
    pub action: String,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub shares: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub price_per_share: Decimal,
    #[sea_orm(column_type = "Decimal(Some((20, 8)))")]
    pub total_cost: Decimal,
    /// "placed" | "filled" | "cancelled" | "failed"
    pub status: String,
    /// "won" | "lost" | null (unresolved)
    pub resolution: Option<String>,
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
