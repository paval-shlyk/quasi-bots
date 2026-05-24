use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PredictionRecords::Table)
                    .if_not_exists()
                    .col(uuid(PredictionRecords::Id).primary_key())
                    .col(string(PredictionRecords::MarketId))
                    .col(string(PredictionRecords::MarketTitle))
                    .col(string(PredictionRecords::Side))
                    .col(string(PredictionRecords::Action))
                    .col(decimal_len(PredictionRecords::Shares, 20, 8))
                    .col(decimal_len(PredictionRecords::PricePerShare, 20, 8))
                    .col(decimal_len(PredictionRecords::TotalCost, 20, 8))
                    .col(string(PredictionRecords::Status))
                    .col(string_null(PredictionRecords::Resolution))
                    .col(text_null(PredictionRecords::LlmRationale))
                    .col(decimal_len_null(PredictionRecords::LlmConfidence, 5, 4))
                    .col(timestamp_with_time_zone(PredictionRecords::CreatedAt))
                    .col(timestamp_with_time_zone(PredictionRecords::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_prediction_records_market_id")
                    .table(PredictionRecords::Table)
                    .col(PredictionRecords::MarketId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_prediction_records_created_at")
                    .table(PredictionRecords::Table)
                    .col(PredictionRecords::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PredictionRecords::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PredictionRecords {
    Table,
    Id,
    MarketId,
    MarketTitle,
    Side,
    Action,
    Shares,
    PricePerShare,
    TotalCost,
    Status,
    Resolution,
    LlmRationale,
    LlmConfidence,
    CreatedAt,
    UpdatedAt,
}
