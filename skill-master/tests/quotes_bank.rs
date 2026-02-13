use skill_master::apply_migrations;
use skill_master::quotes::sync_task::{ZenQuote, insert_new_quotes};
use sqlx::SqlitePool;

async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    apply_migrations(&pool).await;
    pool
}

#[tokio::test]
async fn test_sync_task_inserts_quotes() {
    let pool = setup_db().await;

    let quotes = vec![
        ZenQuote {
            q: "Test Quote 1".to_string(),
            a: "Author 1".to_string(),
        },
        ZenQuote {
            q: "Test Quote 2".to_string(),
            a: "Author 2".to_string(),
        },
    ];

    insert_new_quotes(quotes.clone(), &pool).await.unwrap();

    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM quote")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 2);

    // Verify Author creation
    let author_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM author")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(author_count, 2);
}

#[tokio::test]
async fn test_sync_task_deduplicates_authors() {
    let pool = setup_db().await;

    let quotes = vec![
        ZenQuote {
            q: "Q1".to_string(),
            a: "Same Author".to_string(),
        },
        ZenQuote {
            q: "Q2".to_string(),
            a: "Same Author".to_string(),
        },
    ];

    insert_new_quotes(quotes, &pool).await.unwrap();

    let author_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM author")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(author_count, 1);
}

#[tokio::test]
async fn test_sync_task_ignores_duplicate_quotes() {
    let pool = setup_db().await;

    let quotes = vec![ZenQuote {
        q: "Unique Quote".to_string(),
        a: "Author".to_string(),
    }];

    // First insert
    insert_new_quotes(quotes.clone(), &pool).await.unwrap();

    // Second insert
    insert_new_quotes(quotes.clone(), &pool).await.unwrap();

    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM quote")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 1);
}
