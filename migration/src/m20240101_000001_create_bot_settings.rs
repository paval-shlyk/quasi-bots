use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BotSettings::Table)
                    .if_not_exists()
                    .col(pk_auto(BotSettings::Id))
                    .col(json_binary(BotSettings::SettingsJson))
                    .col(timestamp_with_time_zone(BotSettings::UpdatedAt))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BotSettings::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum BotSettings {
    Table,
    Id,
    SettingsJson,
    UpdatedAt,
}
