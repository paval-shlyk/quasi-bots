use chrono::{DateTime, Datelike, Utc};
use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::mcp::server::SkillMasterMcpServer;

use super::util::json;

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateCategory {
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListEntries {
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    month: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateEntry {
    description: String,
    amount: finance::expenses::NativeCurrency,
    category_id: i64,
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct UpdateEntry {
    entry_id: i64,
    description: String,
    amount: finance::expenses::NativeCurrency,
    category_id: i64,
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EntryId {
    entry_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MonthlyReportQuery {
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    month: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct YearlyReportQuery {
    #[serde(default)]
    year: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WeeklyReportQuery {
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    week: Option<u32>,
}

fn parse_created_at(value: Option<String>) -> Result<DateTime<Utc>, String> {
    match value {
        Some(raw) => DateTime::parse_from_rfc3339(&raw)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| e.to_string()),
        None => Ok(Utc::now()),
    }
}

#[tool_router(router = finance_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "List expense categories")]
    async fn list_categories(&self) -> Result<Json<serde_json::Value>, String> {
        finance::expenses::list_all(self.state.finance_state.pool())
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(description = "Create an expense category")]
    async fn create_category(
        &self,
        Parameters(CreateCategory { name }): Parameters<CreateCategory>,
    ) -> Result<Json<serde_json::Value>, String> {
        finance::expenses::create_new(self.state.finance_state.pool(), &name)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(description = "List expense entries (defaults to last 30 days)")]
    async fn list_entries(
        &self,
        Parameters(ListEntries { year, month }): Parameters<ListEntries>,
    ) -> Result<Json<serde_json::Value>, String> {
        let pool = self.state.finance_state.pool();
        let entries = match (year, month) {
            (Some(year), Some(month)) => {
                finance::expenses::list_by_month(pool, year, month).await
            }
            (Some(year), None) => {
                finance::expenses::list_by_year(pool, year).await
            }
            _ => {
                let now = Utc::now();
                let start = now - chrono::Duration::days(30);
                finance::expenses::list_by_date_range(pool, start, now).await
            }
        };

        entries.map_err(|e| e.to_string()).and_then(json)
    }

    #[tool(description = "Create an expense entry")]
    async fn create_entry(
        &self,
        Parameters(CreateEntry {
            description,
            amount,
            category_id,
            created_at,
        }): Parameters<CreateEntry>,
    ) -> Result<Json<serde_json::Value>, String> {
        let created_at = parse_created_at(created_at)?;
        finance::expenses::insert(
            self.state.finance_state.pool(),
            &description,
            amount,
            category_id,
            created_at,
        )
        .await
        .map_err(|e| e.to_string())
        .and_then(json)
    }

    #[tool(description = "Update an expense entry")]
    async fn update_entry(
        &self,
        Parameters(UpdateEntry {
            entry_id,
            description,
            amount,
            category_id,
            created_at,
        }): Parameters<UpdateEntry>,
    ) -> Result<Json<serde_json::Value>, String> {
        let date = parse_created_at(created_at)?;
        match finance::expenses::update(
            self.state.finance_state.pool(),
            entry_id,
            &description,
            amount,
            category_id,
            date,
        )
        .await
        {
            Ok(Some(entry)) => json(entry),
            Ok(None) => Err("entry not found".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Delete an expense entry")]
    async fn delete_entry(
        &self,
        Parameters(EntryId { entry_id }): Parameters<EntryId>,
    ) -> Result<String, String> {
        match finance::expenses::delete(
            self.state.finance_state.pool(),
            entry_id,
        )
        .await
        {
            Ok(true) => Ok("deleted".into()),
            Ok(false) => Err("entry not found".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Yearly expense report")]
    async fn yearly_report(
        &self,
        Parameters(YearlyReportQuery { year }): Parameters<YearlyReportQuery>,
    ) -> Result<Json<serde_json::Value>, String> {
        let year = year.unwrap_or(Utc::now().year());
        finance::expenses::fetch_year_report(
            self.state.finance_state.pool(),
            year,
        )
        .await
        .map_err(|e| e.to_string())
        .and_then(json)
    }

    #[tool(description = "Monthly expense report")]
    async fn monthly_report(
        &self,
        Parameters(MonthlyReportQuery { year, month }): Parameters<
            MonthlyReportQuery,
        >,
    ) -> Result<Json<serde_json::Value>, String> {
        let now = Utc::now();
        let year = year.unwrap_or(now.year());
        let month = month.unwrap_or(now.month());
        finance::expenses::fetch_monthly_report(
            self.state.finance_state.pool(),
            year,
            month,
        )
        .await
        .map_err(|e| e.to_string())
        .and_then(json)
    }

    #[tool(description = "Weekly expense report")]
    async fn weekly_report(
        &self,
        Parameters(WeeklyReportQuery { year, week }): Parameters<
            WeeklyReportQuery,
        >,
    ) -> Result<Json<serde_json::Value>, String> {
        let now = Utc::now();
        let year = year.unwrap_or(now.year());
        let week = week.unwrap_or(now.iso_week().week());
        finance::expenses::fetch_weekly_report(
            self.state.finance_state.pool(),
            year,
            week,
        )
        .await
        .map_err(|e| e.to_string())
        .and_then(json)
    }
}
