//! Finnhub adapters for analyst price targets and earnings calendar.

use chrono::{Datelike, TimeZone, Utc};
use serde::Deserialize;

use super::providers::{
    EarningsCalendarProvider, EarningsInfo, PriceTargetProvider, PriceTargets,
};

const FINNHUB_BASE: &str = "https://finnhub.io/api/v1";

#[derive(Debug, Clone)]
pub struct FinnhubProvider {
    client: reqwest::Client,
}

impl FinnhubProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        let api_key = api_key.into();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "X-Finnhub-Token",
            reqwest::header::HeaderValue::from_str(&api_key)
                .expect("FINNHUB_API_KEY must be a valid header value"),
        );

        Self {
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .expect("failed to build Finnhub HTTP client"),
        }
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> anyhow::Result<T> {
        let mut url = format!("{FINNHUB_BASE}{path}");
        for (i, (k, v)) in query.iter().enumerate() {
            url.push(if i == 0 { '?' } else { '&' });
            url.push_str(&urlencoding::encode(k));
            url.push('=');
            url.push_str(&urlencoding::encode(v));
        }
        // Auth: default header `X-Finnhub-Token` set in `new`.
        let resp = self.client.get(url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }
}

#[derive(Debug, Deserialize)]
struct FinnhubPriceTarget {
    #[serde(rename = "targetHigh")]
    target_high: Option<f64>,
    #[serde(rename = "targetLow")]
    target_low: Option<f64>,
    #[serde(rename = "targetMean")]
    target_mean: Option<f64>,
    /// Docs: `numberAnalysts` — analysts in the target consensus.
    #[serde(rename = "numberAnalysts")]
    number_analysts: Option<u32>,
}

/// One month of recommendation distribution (`GET /stock/recommendation`).
#[derive(Debug, Deserialize)]
struct FinnhubRecommendation {
    #[serde(rename = "strongBuy")]
    strong_buy: Option<u32>,
    buy: Option<u32>,
    hold: Option<u32>,
    sell: Option<u32>,
    #[serde(rename = "strongSell")]
    strong_sell: Option<u32>,
    #[allow(dead_code)]
    period: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FinnhubEpsEstimateResponse {
    data: Option<Vec<FinnhubEpsEstimateRow>>,
}

#[derive(Debug, Deserialize)]
struct FinnhubEpsEstimateRow {
    #[serde(rename = "epsAvg")]
    eps_avg: Option<f64>,
    #[serde(rename = "numberAnalysts")]
    number_analysts: Option<u32>,
    /// Fiscal year (when present); prefer over parsing `period`.
    year: Option<i32>,
    #[allow(dead_code)]
    period: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FinnhubEarningsCalendar {
    #[serde(rename = "earningsCalendar")]
    earnings_calendar: Option<Vec<FinnhubEarningsRow>>,
}

#[derive(Debug, Deserialize)]
struct FinnhubEarningsRow {
    date: Option<String>,
    #[serde(rename = "epsEstimate")]
    eps_estimate: Option<f64>,
    #[serde(rename = "epsActual")]
    eps_actual: Option<f64>,
    symbol: Option<String>,
}

impl PriceTargetProvider for FinnhubProvider {
    async fn targets(&self, symbol: &str) -> anyhow::Result<PriceTargets> {
        // Price target is required; recommendation + EPS estimates soft-fill
        // (often premium / missing on free tier).
        let pt_q = [("symbol", symbol)];
        let rec_q = [("symbol", symbol)];
        let eps_q = [("symbol", symbol), ("freq", "annual")];
        let (pt_res, rec_res, eps_res) = tokio::join!(
            self.get_json::<FinnhubPriceTarget>("/stock/price-target", &pt_q),
            self.get_json::<Vec<FinnhubRecommendation>>(
                "/stock/recommendation",
                &rec_q,
            ),
            self.get_json::<FinnhubEpsEstimateResponse>(
                "/stock/eps-estimate",
                &eps_q,
            ),
        );

        let raw = pt_res.map_err(|e| {
            anyhow::anyhow!("finnhub price target for {symbol}: {e}")
        })?;

        let (recommendation_mean, recommendation_key) = rec_res
            .ok()
            .and_then(|rows| recommendation_from_trends(&rows))
            .unzip();

        let (eps_cy, eps_ny, eps_analysts, eps_growth) = eps_res
            .ok()
            .map(|body| eps_from_annual(body.data.unwrap_or_default()))
            .unwrap_or((None, None, None, None));

        Ok(PriceTargets {
            mean: raw.target_mean,
            high: raw.target_high,
            low: raw.target_low,
            number_of_analysts: raw.number_analysts,
            recommendation_mean,
            recommendation_key,
            upside_pct: None,
            eps_estimate_current_year: eps_cy,
            eps_estimate_next_year: eps_ny,
            eps_growth_current_year: eps_growth,
            eps_estimate_analysts: eps_analysts,
            source: "finnhub".into(),
        })
    }
}

/// Latest recommendation row → (mean on 1–5 scale, dominant key).
fn recommendation_from_trends(
    rows: &[FinnhubRecommendation],
) -> Option<(f64, String)> {
    // API returns newest period first.
    let r = rows.first()?;
    let sb = r.strong_buy.unwrap_or(0) as f64;
    let b = r.buy.unwrap_or(0) as f64;
    let h = r.hold.unwrap_or(0) as f64;
    let s = r.sell.unwrap_or(0) as f64;
    let ss = r.strong_sell.unwrap_or(0) as f64;
    let total = sb + b + h + s + ss;
    if total <= 0.0 {
        return None;
    }
    // Align with Yahoo-style scale: 1 = strong buy … 5 = strong sell.
    let mean = (sb * 1.0 + b * 2.0 + h * 3.0 + s * 4.0 + ss * 5.0) / total;

    let key = [
        ("strong_buy", sb),
        ("buy", b),
        ("hold", h),
        ("sell", s),
        ("strong_sell", ss),
    ]
    .into_iter()
    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    .map(|(k, _)| k.to_string())?;

    Some((mean, key))
}

/// Pick current / next fiscal year EPS from annual estimate rows.
fn eps_from_annual(
    mut rows: Vec<FinnhubEpsEstimateRow>,
) -> (Option<f64>, Option<f64>, Option<u32>, Option<f64>) {
    rows.sort_by_key(|r| r.year.unwrap_or(i32::MAX));
    let now_y = Utc::now().year();

    // Prefer years >= current calendar year (forward estimates).
    let mut forward: Vec<_> = rows
        .into_iter()
        .filter(|r| r.year.map(|y| y >= now_y).unwrap_or(false))
        .collect();
    if forward.is_empty() {
        return (None, None, None, None);
    }
    forward.sort_by_key(|r| r.year.unwrap_or(i32::MAX));

    let cy = forward.first();
    let ny = forward.get(1);

    let eps_cy = cy.and_then(|r| r.eps_avg);
    let eps_ny = ny.and_then(|r| r.eps_avg);
    let analysts = cy.and_then(|r| r.number_analysts);
    let growth = match (eps_cy, eps_ny) {
        (Some(c), Some(n)) if c.abs() > f64::EPSILON => Some((n - c) / c),
        _ => None,
    };

    (eps_cy, eps_ny, analysts, growth)
}

impl EarningsCalendarProvider for FinnhubProvider {
    async fn earnings(&self, symbol: &str) -> anyhow::Result<EarningsInfo> {
        // Window: 1y past → 1y future
        let from = (Utc::now() - chrono::Duration::days(365))
            .format("%Y-%m-%d")
            .to_string();
        let to = (Utc::now() + chrono::Duration::days(365))
            .format("%Y-%m-%d")
            .to_string();

        let cal: FinnhubEarningsCalendar = self
            .get_json(
                "/calendar/earnings",
                &[("symbol", symbol), ("from", &from), ("to", &to)],
            )
            .await?;

        let rows = cal.earnings_calendar.unwrap_or_default();
        let mut dated: Vec<_> = rows
            .into_iter()
            .filter(|r| {
                r.symbol
                    .as_deref()
                    .map(|s| s.eq_ignore_ascii_case(symbol))
                    .unwrap_or(true)
            })
            .filter_map(|r| {
                let d = r.date.as_deref()?;
                let dt = parse_date(d)?;
                Some((dt, r))
            })
            .collect();

        dated.sort_by_key(|(dt, _)| *dt);

        let now = Utc::now();
        let last = dated.iter().rev().find(|(dt, _)| *dt <= now);
        let next = dated.iter().find(|(dt, _)| *dt > now);

        Ok(EarningsInfo {
            next_report_at: next.map(|(dt, _)| *dt),
            last_report_at: last.map(|(dt, _)| *dt),
            eps_estimate: next
                .and_then(|(_, r)| r.eps_estimate)
                .or_else(|| last.and_then(|(_, r)| r.eps_estimate)),
            eps_actual: last.and_then(|(_, r)| r.eps_actual),
            source: "finnhub".into(),
        })
    }
}

fn parse_date(s: &str) -> Option<chrono::DateTime<Utc>> {
    let naive = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    naive
        .and_hms_opt(0, 0, 0)
        .map(|ndt| Utc.from_utc_datetime(&ndt))
}
