use crate::expenses::{NativeCurrency, chart, entry};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema,
)]
pub struct CategoryTotal {
    pub category_id: i64,
    pub category_name: String,
    pub total: NativeCurrency,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema,
)]
pub struct MonthlyReport {
    pub year: i32,
    pub month: u32,
    pub total: NativeCurrency,
    pub by_category: Vec<CategoryTotal>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema,
)]
pub struct MonthData {
    pub month: u32,
    pub total: NativeCurrency,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema,
)]
pub struct YearReport {
    pub year: i32,
    pub total: NativeCurrency,
    pub by_category: Vec<CategoryTotal>,
    pub by_month: Vec<MonthData>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema,
)]
pub struct WeekData {
    pub week: u32,
    pub total: NativeCurrency,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema,
)]
pub struct WeeklyReport {
    pub year: i32,
    pub week: u32,
    pub total: NativeCurrency,
    pub by_category: Vec<CategoryTotal>,
}

pub async fn fetch_monthly_report(
    pool: &SqlitePool,
    year: i32,
    month: u32,
) -> sqlx::Result<MonthlyReport> {
    let entries = entry::list_by_month(pool, year, month).await?;

    let total: NativeCurrency = entries.iter().map(|e| e.amount).sum();

    let mut category_totals: std::collections::HashMap<
        i64,
        (String, NativeCurrency),
    > = std::collections::HashMap::new();

    for e in entries {
        let entry = category_totals
            .entry(e.category_id)
            .or_insert((e.category_name.clone(), 0));
        entry.1 += e.amount;
    }

    let by_category: Vec<CategoryTotal> = category_totals
        .into_iter()
        .map(|(id, (name, total))| CategoryTotal {
            category_id: id,
            category_name: name,
            total,
        })
        .collect();

    Ok(MonthlyReport {
        year,
        month,
        total,
        by_category,
    })
}

pub async fn fetch_year_report(
    pool: &SqlitePool,
    year: i32,
) -> sqlx::Result<YearReport> {
    let entries = entry::list_by_year(pool, year).await?;

    let total: NativeCurrency = entries.iter().map(|e| e.amount).sum();

    let mut category_totals: std::collections::HashMap<
        i64,
        (String, NativeCurrency),
    > = std::collections::HashMap::new();
    let mut month_totals: std::collections::HashMap<u32, NativeCurrency> =
        std::collections::HashMap::new();

    for e in entries {
        let cat_entry = category_totals
            .entry(e.category_id)
            .or_insert((e.category_name.clone(), 0));
        cat_entry.1 += e.amount;

        let month = e.date.format("%m").to_string().parse::<u32>().unwrap_or(1);
        *month_totals.entry(month).or_insert(0) += e.amount;
    }

    let by_category: Vec<CategoryTotal> = category_totals
        .into_iter()
        .map(|(id, (name, total))| CategoryTotal {
            category_id: id,
            category_name: name,
            total,
        })
        .collect();

    let by_month: Vec<MonthData> = month_totals
        .into_iter()
        .map(|(month, total)| MonthData { month, total })
        .collect();

    Ok(YearReport {
        year,
        total,
        by_category,
        by_month,
    })
}

pub async fn fetch_weekly_report(
    pool: &SqlitePool,
    year: i32,
    week: u32,
) -> sqlx::Result<WeeklyReport> {
    let entries = entry::list_by_week(pool, year, week).await?;

    let total: NativeCurrency = entries.iter().map(|e| e.amount).sum();

    let mut category_totals: std::collections::HashMap<
        i64,
        (String, NativeCurrency),
    > = std::collections::HashMap::new();

    for e in entries {
        let entry = category_totals
            .entry(e.category_id)
            .or_insert((e.category_name.clone(), 0));
        entry.1 += e.amount;
    }

    let by_category: Vec<CategoryTotal> = category_totals
        .into_iter()
        .map(|(id, (name, total))| CategoryTotal {
            category_id: id,
            category_name: name,
            total,
        })
        .collect();

    Ok(WeeklyReport {
        year,
        week,
        total,
        by_category,
    })
}

pub fn generate_monthly_chart(report: &MonthlyReport) -> Option<Vec<u8>> {
    if report.by_category.is_empty() {
        return None;
    }
    let title = format!("Expenses {}/{}", report.year, report.month);
    chart::create_bar_chart(&report.by_category, &title).ok()
}

pub fn generate_year_chart(report: &YearReport) -> Option<Vec<u8>> {
    if report.by_month.is_empty() {
        return None;
    }
    let data: Vec<(u32, u64)> =
        report.by_month.iter().map(|m| (m.month, m.total)).collect();
    let title = format!("Expenses {}", report.year);
    chart::create_year_chart(&data, &title).ok()
}

pub fn generate_weekly_chart(report: &WeeklyReport) -> Option<Vec<u8>> {
    if report.by_category.is_empty() {
        return None;
    }
    let title = format!("Expenses {} W{}", report.year, report.week);
    chart::create_bar_chart(&report.by_category, &title).ok()
}
