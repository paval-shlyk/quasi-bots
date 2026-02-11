#![allow(unused)]
use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use reqwest::StatusCode;

use crate::{
    AppState,
    finance::{metrics::AnalysisConfig, model::TrackingAsset},
};

#[derive(serde::Deserialize)]
pub struct AssetQuery {
    pub asset: String,
}

///Generate report for given asset using it symbol
pub async fn get_report(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<AssetQuery>,
) -> impl IntoResponse {
    let config = AnalysisConfig::default();

    match super::metrics::analyze_asset(&query.asset, &config).await {
        Some(report) => (StatusCode::OK, Json(report)).into_response(),
        None => (StatusCode::BAD_REQUEST).into_response(),
    }
}

pub async fn get_tracking_assets(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

async fn build_report(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<TrackingAsset>> {
    let assets = sqlx::query_as!(
        TrackingAsset,
        r#"
            SELECT symbol, name
            FROM asset
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(assets)
}

pub async fn post_tracking_asset() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

// return news about hype assets
// returns recommendation list based on news fetched
pub async fn get_market_recommendations(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let source = state.config.investment_rss_sources[0].clone();
    let client = reqwest::Client::new();

    match fetch_popular_assets(&client, source).await {
        Ok(assets) => (StatusCode::OK, Json(assets)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

async fn fetch_popular_assets(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> anyhow::Result<Vec<String>> {
    let resp = client.get(url).send().await?;
    let content = resp.bytes().await?;
    let feed = feed_rs::parser::parse(content.as_ref())?;

    let assets = feed
        .entries
        .into_iter()
        .filter_map(|e| e.title)
        .map(|t| t.content)
        .collect::<Vec<_>>();

    Ok(assets)

    // let e = &feed.entries[0];
    // if e.links.is_empty() {
    //     return Ok(vec![]);
    // }
    // let link = &e.links[0].href;
    //
    //
    // let page = client
    //     .get(link)
    //     .header(
    //         "User-Agent",
    //         "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
    //     )
    //     .send()
    //     .await?;
    // let page_content = page.text().await?;
    //
    // let document = scraper::Html::parse_document(&page_content);
    // let selector =
    //     scraper::Selector::parse("script[type='application/ld+json']").unwrap();
    //
    // let mut results = Vec::new();
    // for element in document.select(&selector) {
    //     results.push(element.inner_html());
    // }
    //
    // Ok(results)
}
