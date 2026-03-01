use sqlx::SqlitePool;

async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../skill-master/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn test_expense_crud() {
    let pool = setup_db().await;

    finance::expenses::init_predefined(&pool).await.unwrap();

    let categories = finance::expenses::list_all(&pool).await.unwrap();
    assert!(!categories.is_empty());
    let food_category = categories.iter().find(|c| c.name == "Food").unwrap();

    let entry = finance::expenses::insert(
        &pool,
        "Test expense",
        1000,
        food_category.id,
    )
    .await
    .unwrap();
    assert_eq!(entry.description, "Test expense");
    assert_eq!(entry.amount, 1000);
    assert_eq!(entry.category_id, food_category.id);

    let now = chrono::Utc::now();
    let entries = finance::expenses::list_by_month(
        &pool,
        now.format("%Y").to_string().parse().unwrap(),
        now.format("%m").to_string().parse().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].category_name, "Food");

    let custom_cat = finance::expenses::create_new(&pool, "Custom Category")
        .await
        .unwrap();
    assert_eq!(custom_cat.name, "Custom Category");

    let report = finance::expenses::fetch_monthly_report(
        &pool,
        now.format("%Y").to_string().parse().unwrap(),
        now.format("%m").to_string().parse().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(report.total, 1000);
    assert_eq!(report.by_category.len(), 1);
    assert_eq!(report.by_category[0].category_name, "Food");
}
