use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};

use utoipa::{IntoParams, ToSchema};

use crate::expenses::{
    NativeCurrency, category, entry,
    report::{self, MonthlyReport, YearReport},
    Category, ExpenseEntry, ExpenseEntryWithCategory,
};

use crate::FinanceState;

pub fn router() -> Router<FinanceState> {
    Router::new()
        .route("/categories", get(list_categories))
        .route("/categories", post(create_category))
        .route("/entries", get(list_entries))
        .route("/entries", post(create_entry))
        .route("/report/monthly", get(monthly_report))
        .route("/report/yearly", get(yearly_report))
}

#[utoipa::path(
    get,
    path = "/expenses/categories",
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
    path = "/expenses/categories",
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

#[derive(serde::Deserialize, ToSchema)]
struct CreateCategoryRequest {
    name: String,
}

#[utoipa::path(
    get,
    path = "/expenses/entries",
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

#[derive(IntoParams, serde::Deserialize)]
struct ListEntriesParams {
    year: Option<i32>,
    month: Option<u32>,
}

#[utoipa::path(
    post,
    path = "/expenses/entries",
    request_body = CreateEntryRequest,
    responses(
        (status = 201, body = ExpenseEntry)
    )
)]
async fn create_entry(
    State(state): State<FinanceState>,
    Json(payload): Json<CreateEntryRequest>,
) -> impl IntoResponse {
    match entry::insert(
        &state.pool,
        &payload.description,
        payload.amount,
        payload.category_id,
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

#[derive(serde::Deserialize, ToSchema)]
struct CreateEntryRequest {
    description: String,
    amount: NativeCurrency,
    category_id: i64,
}

#[utoipa::path(
    get,
    path = "/expenses/report/monthly",
    params(ReportParams),
    responses(
        (status = 200, body = MonthlyReport)
    )
)]
async fn monthly_report(
    State(state): State<FinanceState>,
    Query(params): Query<ReportParams>,
) -> impl IntoResponse {
    let now = chrono::Utc::now();
    let year = params
        .year
        .unwrap_or(now.format("%Y").to_string().parse().unwrap_or(2026));
    let month = params
        .month
        .unwrap_or(now.format("%m").to_string().parse().unwrap_or(3));

    match report::fetch_monthly_report(&state.pool, year, month).await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/expenses/report/yearly",
    params(ReportParams),
    responses(
        (status = 200, body = YearReport)
    )
)]
async fn yearly_report(
    State(state): State<FinanceState>,
    Query(params): Query<ReportParams>,
) -> impl IntoResponse {
    let now = chrono::Utc::now();
    let year = params
        .year
        .unwrap_or(now.format("%Y").to_string().parse().unwrap_or(2026));

    match report::fetch_year_report(&state.pool, year).await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(IntoParams, serde::Deserialize)]
struct ReportParams {
    year: Option<i32>,
    month: Option<u32>,
}
