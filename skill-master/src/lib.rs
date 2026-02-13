pub mod finance;
pub mod news;
pub mod quotes;
pub mod routes;
pub mod search;

mod state;

mod config;

use std::sync::Arc;

pub use config::*;
pub use state::*;

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

pub async fn app_state(config: Config) -> AppState {
    let pool = connect_db(&config.db_file).await;
    apply_migrations(&pool).await;

    let knowledge_state = knowledge::connect(pool.clone())
        .await
        .expect("Failed to connect knowledge database");

    let topics = knowledge::fetch_topics(&pool)
        .await
        .expect("Failed to fetch topics");

    if topics.is_empty() {
        knowledge::refresh_from_files(
            &knowledge_state,
            &config.knowledge.database_file,
        )
        .await
        .expect("Failed to refresh empty knowledge database");
    }

    AppState {
        config: Arc::new(config),
        pool,
        needs_more_quotes: Arc::new(tokio::sync::Notify::new()),
        knowledge_state,
    }
}
