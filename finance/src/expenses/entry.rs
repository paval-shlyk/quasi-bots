use crate::expenses::NativeCurrency;
use chrono::{DateTime, Utc};

#[derive(
    Debug,
    Clone,
    sqlx::FromRow,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub struct ExpenseEntry {
    pub id: i64,
    pub description: String,
    pub date: DateTime<Utc>,
    pub amount: NativeCurrency,
    pub category_id: i64,
}

#[derive(
    Debug,
    sqlx::FromRow,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub struct ExpenseEntryWithCategory {
    pub id: i64,
    pub description: String,
    pub date: DateTime<Utc>,
    pub amount: NativeCurrency,
    pub category_id: i64,
    pub category_name: String,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct ExpenseEntryList {
    pub entries: Vec<ExpenseEntryWithCategory>,
}

pub async fn insert(
    pool: &sqlx::SqlitePool,
    description: &str,
    amount: NativeCurrency,
    category_id: i64,
    date: chrono::DateTime<chrono::Utc>,
) -> sqlx::Result<ExpenseEntry> {
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
        date,
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
        date,
        amount: amount as u64,
        category_id,
    })
}

pub async fn update(
    pool: &sqlx::SqlitePool,
    id: i64,
    description: &str,
    amount: NativeCurrency,
    category_id: i64,
    date: chrono::DateTime<chrono::Utc>,
) -> sqlx::Result<Option<ExpenseEntry>> {
    let amount = amount as i64;

    let maybe_entry = sqlx::query_as!(
            ExpenseEntry,
            r#"
            UPDATE expense_entries
            SET description = ?, amount = ?, category_id = ?, date = ?
            WHERE id = ?
            RETURNING id as "id!", description as "description!", date as "date: DateTime<Utc>", amount as "amount: u64", category_id as "category_id!"
            "#,
            description,
            amount,
            category_id,
            date,
            id
        )
        .fetch_optional(pool)
        .await?;

    Ok(maybe_entry)
}

pub async fn delete(pool: &sqlx::SqlitePool, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM expense_entries
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn list_by_date_range(
    pool: &sqlx::SqlitePool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> sqlx::Result<ExpenseEntryList> {
    let entries = sqlx::query_as!(
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
    .await?;

    Ok(ExpenseEntryList { entries })
}

pub async fn list_by_month(
    pool: &sqlx::SqlitePool,
    year: i32,
    month: u32,
) -> sqlx::Result<ExpenseEntryList> {
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
) -> sqlx::Result<ExpenseEntryList> {
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
) -> sqlx::Result<ExpenseEntryList> {
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
