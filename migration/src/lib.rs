pub use sea_orm_migration::prelude::*;

mod m20240101_000001_create_bot_settings;
mod m20240101_000002_create_portfolio_snapshots;
mod m20240101_000003_create_trade_records;
mod m20240101_000004_create_open_positions;
mod m20240101_000005_create_prediction_records;
mod m20240101_000006_create_open_predictions;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240101_000001_create_bot_settings::Migration),
            Box::new(m20240101_000002_create_portfolio_snapshots::Migration),
            Box::new(m20240101_000003_create_trade_records::Migration),
            Box::new(m20240101_000004_create_open_positions::Migration),
            Box::new(m20240101_000005_create_prediction_records::Migration),
            Box::new(m20240101_000006_create_open_predictions::Migration),
        ]
    }
}
