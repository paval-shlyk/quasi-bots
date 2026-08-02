use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OpenPositions::Table)
                    .if_not_exists()
                    .col(uuid(OpenPositions::Id).primary_key())
                    .col(string(OpenPositions::Pair))
                    .col(string(OpenPositions::Side))
                    .col(decimal_len(OpenPositions::EntryPrice, 20, 8))
                    .col(decimal_len(OpenPositions::Quantity, 20, 8))
                    .col(decimal_len(OpenPositions::CurrentPrice, 20, 8))
                    .col(decimal_len(OpenPositions::UnrealizedPnl, 20, 8))
                    .col(decimal_len_null(OpenPositions::StopLossPrice, 20, 8))
                    .col(decimal_len_null(
                        OpenPositions::TakeProfitPrice,
                        20,
                        8,
                    ))
                    .col(decimal_len(OpenPositions::AllocatedCapital, 20, 8))
                    .col(timestamp_with_time_zone(OpenPositions::OpenedAt))
                    .col(timestamp_with_time_zone(OpenPositions::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_open_positions_pair")
                    .table(OpenPositions::Table)
                    .col(OpenPositions::Pair)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OpenPositions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum OpenPositions {
    Table,
    Id,
    Pair,
    Side,
    EntryPrice,
    Quantity,
    CurrentPrice,
    UnrealizedPnl,
    StopLossPrice,
    TakeProfitPrice,
    AllocatedCapital,
    OpenedAt,
    UpdatedAt,
}
