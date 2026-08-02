use sqlx::SqlitePool;

#[derive(
    Debug, Clone, sqlx::FromRow, serde::Serialize, schemars::JsonSchema,
)]
pub struct Category {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct CategoryList {
    pub categories: Vec<Category>,
}

pub const PREDEFINED_CATEGORIES: &[&str] = &[
    "Food",
    "Transport",
    "Utilities",
    "Entertainment",
    "Shopping",
    "Health",
    "Trading Deposit",
    "Other",
];

pub async fn list_all(pool: &SqlitePool) -> sqlx::Result<CategoryList> {
    let categories = sqlx::query_as!(
        Category,
        r#"
        SELECT
            id as "id!", name
        FROM
            expense_categories
        ORDER BY 
            name"#
    )
    .fetch_all(pool)
    .await?;

    Ok(CategoryList { categories })
}

pub async fn find_by_id(
    pool: &SqlitePool,
    id: i64,
) -> sqlx::Result<Option<Category>> {
    sqlx::query_as!(
        Category,
        r#"
        SELECT
            id as "id!", name
        FROM
            expense_categories
        WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await
}

pub async fn create_new(
    pool: &SqlitePool,
    name: &str,
) -> sqlx::Result<Category> {
    let category_id = sqlx::query!(
        r#"
            INSERT INTO expense_categories (name) VALUES (?)
            RETURNING id as "id!"
        "#,
        name
    )
    .fetch_one(pool)
    .await?
    .id;

    Ok(Category {
        id: category_id,
        name: name.to_string(),
    })
}

pub async fn init_predefined(pool: &SqlitePool) -> sqlx::Result<()> {
    for name in PREDEFINED_CATEGORIES {
        sqlx::query!(
            "INSERT OR IGNORE INTO expense_categories (name) VALUES (?)",
            name
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}
