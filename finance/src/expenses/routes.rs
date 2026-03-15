use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Datelike;

use crate::expenses::{
    Category, ExpenseEntry, ExpenseEntryWithCategory, NativeCurrency, category,
    entry,
    report::{self, MonthlyReport, WeeklyReport, YearReport},
};

use crate::FinanceState;

pub fn router() -> Router<FinanceState> {
    Router::new()
        .route("/categories", get(list_categories))
        .route("/categories", post(create_category))
        .route("/entries", get(list_entries))
        .route("/entries", post(create_entry))
        .route(
            "/entries/{entry_id}",
            post(update_entry).delete(delete_entry),
        )
        .route("/report/monthly", get(monthly_report))
        .route("/report/yearly", get(yearly_report))
        .route("/report/weekly", get(weekly_report))
}

#[utoipa::path(
    get,
    path = "/expenses-bank/categories",
    tag = "Finance",
    responses(
        (status = 200, body = Vec<Category>)
    )
)]
async fn list_categories(
    State(state): State<FinanceState>,
) -> impl IntoResponse {
    match category::list_all(&state.pool).await {
        Ok(categories) => (StatusCode::OK, Json(categories)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/expenses-bank/categories",
    tag = "Finance",
    request_body = CreateCategoryRequest,
    responses(
        (status = 201, body = Category)
    )
)]
async fn create_category(
    State(state): State<FinanceState>,
    Json(payload): Json<CreateCategoryRequest>,
) -> impl IntoResponse {
    match category::create_new(&state.pool, &payload.name).await {
        Ok(category) => (StatusCode::CREATED, Json(category)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct CreateCategoryRequest {
    #[schema(example = "Food")]
    name: String,
}

#[utoipa::path(
    get,
    path = "/expenses-bank/entries",
    tag = "Finance",
    params(ListEntriesParams),
    responses(
        (status = 200, body = Vec<ExpenseEntryWithCategory>)
    )
)]
async fn list_entries(
    State(state): State<FinanceState>,
    Query(params): Query<ListEntriesParams>,
) -> impl IntoResponse {
    let entries = match (params.year, params.month) {
        (Some(year), Some(month)) => {
            entry::list_by_month(&state.pool, year, month).await
        }
        (Some(year), None) => entry::list_by_year(&state.pool, year).await,
        _ => {
            let now = chrono::Utc::now();
            let start = now - chrono::Duration::days(30);
            entry::list_by_date_range(&state.pool, start, now).await
        }
    };

    match entries {
        Ok(entries) => (StatusCode::OK, Json(entries)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(utoipa::IntoParams, serde::Deserialize)]
struct ListEntriesParams {
    #[param(minimum = 2000)]
    year: Option<i32>,
    #[param(minimum = 1, maximum = 12)]
    month: Option<u32>,
}

#[utoipa::path(
    post,
    path = "/expenses-bank/entries",
    tag = "Finance",
    request_body = CreateEntryRequest,
    responses(
        (status = 201, body = ExpenseEntry)
    )
)]
async fn create_entry(
    State(state): State<FinanceState>,
    Json(payload): Json<CreateEntryRequest>,
) -> impl IntoResponse {
    let created_at = payload.created_at.unwrap_or_else(chrono::Utc::now);

    match entry::insert(
        &state.pool,
        &payload.description,
        payload.amount,
        payload.category_id,
        created_at,
    )
    .await
    {
        Ok(entry) => (StatusCode::CREATED, Json(entry)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/expenses-bank/entries/{entry_id}",
    tag = "Finance",
    params(
        ("entry_id" = i64, Path, description = "Expense entry ID to delete"),
    ),
    responses(
        (status = 204, description = "Entry deleted successfully"),
        (status = 404, description = "Entry not found")
    )
)]
async fn delete_entry(
    State(state): State<FinanceState>,
    Path(entry_id): Path<i64>,
) -> impl IntoResponse {
    match entry::delete(&state.pool, entry_id).await {
        Ok(is_deleted) => if is_deleted {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::NOT_FOUND
        }
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/expenses-bank/entries/{entry_id}",
    tag = "Finance",
    request_body = CreateEntryRequest,
    params(
        ("entry_id" = i64, Path, description = "Expense entry ID"),
    ),
    responses(
        (status = 200, body = ExpenseEntry),
        (status = 404, description = "Entry not found")
    )
)]
async fn update_entry(
    State(state): State<FinanceState>,
    Path(entry_id): Path<i64>,
    Json(payload): Json<CreateEntryRequest>,
) -> impl IntoResponse {
    let date = payload.created_at.unwrap_or_else(chrono::Utc::now);

    match entry::update(
        &state.pool,
        entry_id,
        &payload.description,
        payload.amount,
        payload.category_id,
        date,
    )
    .await
    {
        Ok(Some(entry)) => (StatusCode::OK, Json(entry)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Entry not found").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct CreateEntryRequest {
    #[schema(example = "Grocery shopping")]
    description: String,
    #[schema(minimum = 1)]
    amount: NativeCurrency,
    #[schema(minimum = 1)]
    category_id: i64,
    #[schema(value_type = Option<String>, format = Date)]
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(utoipa::IntoParams, serde::Deserialize)]
struct ReportParams {
    #[param(minimum = 2000)]
    year: Option<i32>,
    #[param(minimum = 1, maximum = 12)]
    month: Option<u32>,
    #[param(example = "png")]
    format: Option<String>,
}

#[utoipa::path(
    get,
    path = "/expenses-bank/report/monthly",
    tag = "Finance",
    params(ReportParams),
    responses(
        (status = 200, body = MonthlyReport),
        (status = 200, description = "PNG chart", content_type = "image/png")
    )
)]
async fn monthly_report(
    State(state): State<FinanceState>,
    Query(params): Query<ReportParams>,
) -> impl IntoResponse {
    let now = chrono::Utc::now();
    let year = params.year.unwrap_or(now.year());
    let month = params.month.unwrap_or(now.month());

    match report::fetch_monthly_report(&state.pool, year, month).await {
        Ok(report) => {
            if params.format.as_deref() == Some("png") {
                match report::generate_monthly_chart(&report) {
                    Some(png) => (
                        StatusCode::OK,
                        [(axum::http::header::CONTENT_TYPE, "image/png")],
                        png,
                    )
                        .into_response(),
                    None => (StatusCode::NO_CONTENT).into_response(),
                }
            } else {
                (StatusCode::OK, Json(report)).into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/expenses-bank/report/yearly",
    tag = "Finance",
    params(ReportParams),
    responses(
        (status = 200, body = YearReport),
        (status = 200, description = "PNG chart", content_type = "image/png")
    )
)]
async fn yearly_report(
    State(state): State<FinanceState>,
    Query(params): Query<ReportParams>,
) -> impl IntoResponse {
    let now = chrono::Utc::now();
    let year = params.year.unwrap_or(now.year());

    match report::fetch_year_report(&state.pool, year).await {
        Ok(report) => {
            if params.format.as_deref() == Some("png") {
                match report::generate_year_chart(&report) {
                    Some(png) => (
                        StatusCode::OK,
                        [(axum::http::header::CONTENT_TYPE, "image/png")],
                        png,
                    )
                        .into_response(),
                    None => (StatusCode::NO_CONTENT).into_response(),
                }
            } else {
                (StatusCode::OK, Json(report)).into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(utoipa::IntoParams, serde::Deserialize)]
struct WeeklyReportParams {
    #[param(minimum = 2000)]
    year: Option<i32>,
    #[param(minimum = 1, maximum = 53)]
    week: Option<u32>,
    #[param(example = "png")]
    format: Option<String>,
}

#[utoipa::path(
    get,
    path = "/expenses-bank/report/weekly",
    tag = "Finance",
    params(WeeklyReportParams),
    responses(
        (status = 200, body = WeeklyReport),
        (status = 200, description = "PNG chart", content_type = "image/png")
    )
)]
async fn weekly_report(
    State(state): State<FinanceState>,
    Query(params): Query<WeeklyReportParams>,
) -> impl IntoResponse {
    let now = chrono::Utc::now();
    let year = params.year.unwrap_or(now.year());
    let week = params.week.unwrap_or(now.iso_week().week());

    match report::fetch_weekly_report(&state.pool, year, week).await {
        Ok(report) => {
            if params.format.as_deref() == Some("png") {
                match report::generate_weekly_chart(&report) {
                    Some(png) => (
                        StatusCode::OK,
                        [(axum::http::header::CONTENT_TYPE, "image/png")],
                        png,
                    )
                        .into_response(),
                    None => (StatusCode::NO_CONTENT).into_response(),
                }
            } else {
                (StatusCode::OK, Json(report)).into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
