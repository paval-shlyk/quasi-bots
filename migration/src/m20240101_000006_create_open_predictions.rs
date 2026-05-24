use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OpenPredictions::Table)
                    .if_not_exists()
                    .col(uuid(OpenPredictions::Id).primary_key())
                    .col(string(OpenPredictions::MarketId))
                    .col(string(OpenPredictions::MarketTitle))
                    .col(string(OpenPredictions::Side))
                    .col(decimal_len(OpenPredictions::Shares, 20, 8))
                    .col(decimal_len(OpenPredictions::AvgPrice, 20, 8))
                    .col(decimal_len(OpenPredictions::CurrentPrice, 20, 8))
                    .col(decimal_len(OpenPredictions::UnrealizedPnl, 20, 8))
                    .col(decimal_len(OpenPredictions::AllocatedCapital, 20, 8))
                    .col(timestamp_with_time_zone(OpenPredictions::OpenedAt))
                    .col(timestamp_with_time_zone(OpenPredictions::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_open_predictions_market_id")
                    .table(OpenPredictions::Table)
                    .col(OpenPredictions::MarketId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OpenPredictions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum OpenPredictions {
    Table,
    Id,
    MarketId,
    MarketTitle,
    Side,
    Shares,
    AvgPrice,
    CurrentPrice,
    UnrealizedPnl,
    AllocatedCapital,
    OpenedAt,
    UpdatedAt,
}
