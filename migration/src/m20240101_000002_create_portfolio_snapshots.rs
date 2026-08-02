use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PortfolioSnapshots::Table)
                    .if_not_exists()
                    .col(uuid(PortfolioSnapshots::Id).primary_key())
                    .col(decimal_len(PortfolioSnapshots::TotalBalance, 20, 8))
                    .col(decimal_len(
                        PortfolioSnapshots::AvailableBalance,
                        20,
                        8,
                    ))
                    .col(decimal_len(
                        PortfolioSnapshots::TradingAllocated,
                        20,
                        8,
                    ))
                    .col(decimal_len(
                        PortfolioSnapshots::PolymarketAllocated,
                        20,
                        8,
                    ))
                    .col(decimal_len(PortfolioSnapshots::UnrealizedPnl, 20, 8))
                    .col(decimal_len(PortfolioSnapshots::RealizedPnl, 20, 8))
                    .col(string(PortfolioSnapshots::BaseCurrency))
                    .col(timestamp_with_time_zone(
                        PortfolioSnapshots::SnapshotAt,
                    ))
                    .col(timestamp_with_time_zone(
                        PortfolioSnapshots::CreatedAt,
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_portfolio_snapshots_snapshot_at")
                    .table(PortfolioSnapshots::Table)
                    .col(PortfolioSnapshots::SnapshotAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop().table(PortfolioSnapshots::Table).to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum PortfolioSnapshots {
    Table,
    Id,
    TotalBalance,
    AvailableBalance,
    TradingAllocated,
    PolymarketAllocated,
    UnrealizedPnl,
    RealizedPnl,
    BaseCurrency,
    SnapshotAt,
    CreatedAt,
}
