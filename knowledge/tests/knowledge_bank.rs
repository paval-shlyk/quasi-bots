use knowledge::{add_new_entry, fetch_random_entry, set_entry_affinity};
use sqlx::SqlitePool;

async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    // Path relative to knowledge/Cargo.toml when running from crate root,
    // but here we are in knowledge/tests/ ..
    // Wait, sqlx::migrate! runs at compile time relative to the file where it is invoked.
    // So relative to knowledge/tests/knowledge_bank.rs, the migrations are at ../../skill-master/migrations
    sqlx::migrate!("../skill-master/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn test_cyclic_coverage() {
    let pool = setup_db().await;

    let topic_id = 1;
    let topic_id_db = topic_id as i64;
    sqlx::query!(
        "INSERT INTO topic (id, name, is_used) VALUES (?, ?, ?)",
        topic_id_db,
        "test_topic",
        true
    )
    .execute(&pool)
    .await
    .unwrap();

    // Add 2 entries
    add_new_entry(&pool, topic_id, "Q1".into(), "A1".into(), vec![])
        .await
        .unwrap();
    add_new_entry(&pool, topic_id, "Q2".into(), "A2".into(), vec![])
        .await
        .unwrap();

    // Fetch 1st
    let e1 = fetch_random_entry(&pool).await.unwrap();

    // Fetch 2nd
    let e2 = fetch_random_entry(&pool).await.unwrap();

    assert_ne!(e1.question, e2.question);

    // Fetch 3rd - should reset and be one of the previous
    let e3 = fetch_random_entry(&pool).await.unwrap();
    assert!(e3.question == e1.question || e3.question == e2.question);
}

#[tokio::test]
async fn test_affinity_functionality_bypass_limit() {
    let pool = setup_db().await;

    let topic_id = 1;
    let topic_id_db = topic_id as i64;
    sqlx::query!(
        "INSERT INTO topic (id, name, is_used) VALUES (?, ?, ?)",
        topic_id_db,
        "test_topic",
        true
    )
    .execute(&pool)
    .await
    .unwrap();

    // Add entry
    add_new_entry(&pool, topic_id, "Q1".into(), "A1".into(), vec![])
        .await
        .unwrap();

    let entry_name: String =
        sqlx::query_scalar!("SELECT name FROM entry WHERE question = 'Q1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    // Set affinity to 1 day
    set_entry_affinity(entry_name, 1, &pool).await.unwrap();

    // Fetch it once - marks reviewed and triggers disable
    let e1 = fetch_random_entry(&pool).await.unwrap();
    assert_eq!(e1.question, "Q1");

    // Fetch again - should fail because entry is disabled for 1 day
    // fetch_random_entry resets is_reviewed=FALSE, but disabled_until is still set by trigger
    // so it finds nothing and errors.
    let result = fetch_random_entry(&pool).await;
    assert!(result.is_err());

    // Bypass time limit: manually update disabled_until to the past
    // This mocks the passage of time
    sqlx::query!(
        "UPDATE entry SET disabled_until = datetime('now', '-1 hour')"
    )
    .execute(&pool)
    .await
    .unwrap();

    // Fetch again - should succeed now
    let e2 = fetch_random_entry(&pool).await.unwrap();
    assert_eq!(e2.question, "Q1");
}
