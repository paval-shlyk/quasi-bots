use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TradeRecords::Table)
                    .if_not_exists()
                    .col(uuid(TradeRecords::Id).primary_key())
                    .col(string(TradeRecords::Pair))
                    .col(string(TradeRecords::Side))
                    .col(string(TradeRecords::OrderType))
                    .col(decimal_len(TradeRecords::Quantity, 20, 8))
                    .col(decimal_len(TradeRecords::Price, 20, 8))
                    .col(decimal_len(TradeRecords::FilledQuantity, 20, 8))
                    .col(decimal_len(TradeRecords::AvgFillPrice, 20, 8))
                    .col(decimal_len(TradeRecords::Fee, 20, 8))
                    .col(string(TradeRecords::Status))
                    .col(text_null(TradeRecords::LlmRationale))
                    .col(decimal_len_null(TradeRecords::LlmConfidence, 5, 4))
                    .col(timestamp_with_time_zone(TradeRecords::CreatedAt))
                    .col(timestamp_with_time_zone(TradeRecords::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_trade_records_pair")
                    .table(TradeRecords::Table)
                    .col(TradeRecords::Pair)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_trade_records_created_at")
                    .table(TradeRecords::Table)
                    .col(TradeRecords::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TradeRecords::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum TradeRecords {
    Table,
    Id,
    Pair,
    Side,
    OrderType,
    Quantity,
    Price,
    FilledQuantity,
    AvgFillPrice,
    Fee,
    Status,
    LlmRationale,
    LlmConfidence,
    CreatedAt,
    UpdatedAt,
}
