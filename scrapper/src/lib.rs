pub mod config;
pub mod finance;
pub mod model;
pub mod quotes;
pub mod routes;
pub mod rss;
pub mod search;

pub async fn connect_db(db_url: &str) -> sqlx::SqlitePool {
    sqlx::sqlite::SqlitePoolOptions::new()
        .connect(db_url)
        .await
        .expect("Failed to connect to database")
}

pub async fn apply_migrations(pool: &sqlx::SqlitePool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to apply database migrations");
}
