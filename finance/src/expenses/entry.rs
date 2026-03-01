use crate::expenses::NativeCurrency;
use chrono::{DateTime, Utc};

#[derive(
    Debug,
    Clone,
    sqlx::FromRow,
    serde::Serialize,
    serde::Deserialize,
    utoipa::ToSchema,
)]
pub struct ExpenseEntry {
    pub id: i64,
    pub description: String,
    pub date: DateTime<Utc>,
    pub amount: NativeCurrency,
    pub category_id: i64,
}

#[derive(
    Debug, sqlx::FromRow, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
pub struct ExpenseEntryWithCategory {
    pub id: i64,
    pub description: String,
    pub date: DateTime<Utc>,
    pub amount: NativeCurrency,
    pub category_id: i64,
    pub category_name: String,
}

pub async fn insert(
    pool: &sqlx::SqlitePool,
    description: &str,
    amount: NativeCurrency,
    category_id: i64,
) -> sqlx::Result<ExpenseEntry> {
    let now = chrono::Utc::now();
    let amount = amount as i64;

    let expense_id = sqlx::query!(
        r#"
        INSERT INTO expense_entries
            (description, date, amount, category_id)
        VALUES
            (?, ?, ?, ?)
        RETURNING id as "id!"
        "#,
        description,
        now,
        amount,
        category_id
    )
    .fetch_one(pool)
    .await
    .inspect_err(|e| {
        tracing::warn!("Failed to insert new expense: {e}");
    })?
    .id;

    Ok(ExpenseEntry {
        id: expense_id,
        description: description.to_string(),
        date: now,
        amount: amount as u64,
        category_id,
    })
}

pub async fn list_by_date_range(
    pool: &sqlx::SqlitePool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> sqlx::Result<Vec<ExpenseEntryWithCategory>> {
    sqlx::query_as!(
        ExpenseEntryWithCategory,
        r#"
            SELECT 
                e.id as "id!",
                e.description,
                e.date as "date: chrono::DateTime<chrono::Utc>",
                e.amount as "amount: u64",
                e.category_id,
                c.name as category_name
            FROM expense_entries e
            JOIN expense_categories c ON e.category_id = c.id
            WHERE e.date >= ? AND e.date < ?
            ORDER BY e.date DESC
        "#,
        start,
        end
    )
    .fetch_all(pool)
    .await
}

pub async fn list_by_month(
    pool: &sqlx::SqlitePool,
    year: i32,
    month: u32,
) -> sqlx::Result<Vec<ExpenseEntryWithCategory>> {
    let start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let end = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .and_hms_opt(0, 0, 0)
    .unwrap()
    .and_utc();

    list_by_date_range(pool, start, end).await
}

pub async fn list_by_year(
    pool: &sqlx::SqlitePool,
    year: i32,
) -> sqlx::Result<Vec<ExpenseEntryWithCategory>> {
    let start = chrono::NaiveDate::from_ymd_opt(year, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let end = chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    list_by_date_range(pool, start, end).await
}

pub async fn list_by_week(
    pool: &sqlx::SqlitePool,
    year: i32,
    week: u32,
) -> sqlx::Result<Vec<ExpenseEntryWithCategory>> {
    let days_offset = (week.saturating_sub(1)) * 7;
    let start = chrono::NaiveDate::from_ymd_opt(year, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        + chrono::Duration::days(days_offset as i64);

    let end = start + chrono::Duration::days(7);

    list_by_date_range(pool, start, end).await
}
