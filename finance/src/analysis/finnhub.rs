//! Finnhub adapters for analyst price targets and earnings calendar.

use chrono::{TimeZone, Utc};
use serde::Deserialize;

use super::providers::{
    EarningsCalendarProvider, EarningsInfo, PriceTargetProvider, PriceTargets,
};

const FINNHUB_BASE: &str = "https://finnhub.io/api/v1";

#[derive(Debug, Clone)]
pub struct FinnhubProvider {
    api_key: String,
    client: reqwest::Client,
}

impl FinnhubProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: reqwest::Client::new(),
        }
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> anyhow::Result<T> {
        let mut url = format!("{FINNHUB_BASE}{path}?token={}", self.api_key);
        for (k, v) in query {
            url.push('&');
            url.push_str(&urlencoding::encode(k));
            url.push('=');
            url.push_str(&urlencoding::encode(v));
        }
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
        let raw: FinnhubPriceTarget = self
            .get_json("/stock/price-target", &[("symbol", symbol)])
            .await?;

        Ok(PriceTargets {
            mean: raw.target_mean,
            high: raw.target_high,
            low: raw.target_low,
            upside_pct: None,
            source: "finnhub".into(),
        })
    }
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
