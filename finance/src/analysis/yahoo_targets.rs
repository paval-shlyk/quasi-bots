//! Yahoo Finance analyst price targets and pricing estimates (`yfinance-rs`).

use yfinance_rs::core::conversions::money_to_f64;
use yfinance_rs::{Ticker, YfClient};

use super::providers::{PriceTargetProvider, PriceTargets};

/// Fetches consensus analyst price targets and EPS estimates from Yahoo (no API key).
#[derive(Debug, Clone)]
pub struct YahooPriceTargetProvider {
    client: YfClient,
}

impl Default for YahooPriceTargetProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl YahooPriceTargetProvider {
    pub fn new() -> Self {
        Self {
            client: YfClient::default(),
        }
    }
}

impl PriceTargetProvider for YahooPriceTargetProvider {
    async fn targets(&self, symbol: &str) -> anyhow::Result<PriceTargets> {
        let ticker = Ticker::new(&self.client, symbol);

        // Parallel: price target band, recommendation consensus, EPS estimates.
        let (pt_res, rec_res, trend_res) = tokio::join!(
            ticker.analyst_price_target(None),
            ticker.recommendations_summary(),
            ticker.earnings_trend(None),
        );

        let pt = pt_res.map_err(|e| {
            anyhow::anyhow!("yahoo price target for {symbol}: {e}")
        })?;

        let rec = rec_res.ok();
        let trends = trend_res.unwrap_or_default();

        // Yahoo periods: "0y" current year, "+1y" next year (when present).
        let current_year = trends.iter().find(|t| t.period.to_string() == "0y");
        let next_year = trends.iter().find(|t| t.period.to_string() == "+1y");

        let eps_cy = current_year.and_then(|t| {
            t.earnings_estimate.avg.as_ref().map(money_to_f64)
        });
        let eps_ny = next_year.and_then(|t| {
            t.earnings_estimate.avg.as_ref().map(money_to_f64)
        });
        let eps_growth = current_year.and_then(|t| t.earnings_estimate.growth);
        let eps_analysts =
            current_year.and_then(|t| t.earnings_estimate.num_analysts);

        let recommendation_key = rec.as_ref().and_then(|r| {
            r.mean_rating_text
                .clone()
                .or_else(|| recommendation_label(r.mean))
        });

        Ok(PriceTargets {
            mean: pt.mean.as_ref().map(money_to_f64),
            high: pt.high.as_ref().map(money_to_f64),
            low: pt.low.as_ref().map(money_to_f64),
            number_of_analysts: pt.number_of_analysts,
            recommendation_mean: rec.as_ref().and_then(|r| r.mean),
            recommendation_key,
            upside_pct: None,
            eps_estimate_current_year: eps_cy,
            eps_estimate_next_year: eps_ny,
            eps_growth_current_year: eps_growth,
            eps_estimate_analysts: eps_analysts,
            source: "yahoo".into(),
        })
    }
}

/// Map Yahoo-style recommendation mean (1–5) to a short label when text is missing.
fn recommendation_label(mean: Option<f64>) -> Option<String> {
    let m = mean?;
    let label = if m <= 1.5 {
        "strong_buy"
    } else if m <= 2.5 {
        "buy"
    } else if m <= 3.5 {
        "hold"
    } else if m <= 4.5 {
        "sell"
    } else {
        "strong_sell"
    };
    Some(label.into())
}
