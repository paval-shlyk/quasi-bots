use std::time::{Duration, SystemTime};
use ta::Next;
/// This module contains essential logic for finance management
use ta::indicators::{BollingerBands, RelativeStrengthIndex};
use yahoo_finance_api as yahoo;

#[derive(Debug, serde::Serialize)]
pub enum RsiSignal {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, serde::Serialize)]
pub enum Volatility {
    Low,
    High,
}

#[derive(Debug, serde::Serialize)]
pub struct TechnicalReport {
    symbol: String,
    price: f64,
    rsi_14: f64,
    volatility: Volatility,
    signal: RsiSignal,
}

pub struct AnalysisConfig {
    pub rsi_low: f64,
    pub rsi_high: f64,
    pub history_duration: Duration,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            rsi_low: 30.0,
            rsi_high: 70.0,
            history_duration: Duration::from_secs(150 * 24 * 60 * 60), //6 months
        }
    }
}

pub async fn analyze_asset(
    symbol: &str,
    config: &AnalysisConfig,
) -> Option<TechnicalReport> {
    let provider = yahoo::YahooConnector::new()
        .expect("Failed to create Yahoo Finance provider");

    let response = provider
        .get_quote_history(
            symbol,
            calculate_start_time().into(),
            SystemTime::now().into(),
        )
        .await
        .ok()?;

    let quotes = response.quotes().ok()?;
    let mut rsi = RelativeStrengthIndex::new(14).unwrap();
    let mut bb = BollingerBands::new(20, 2.0).unwrap();

    let mut last_rsi = 0.0;
    let mut last_price = 0.0;
    let mut band_width = 0.0;

    for quote in quotes {
        last_price = quote.close;
        last_rsi = rsi.next(quote.close);
        let bb_output = bb.next(quote.close);
        band_width = (bb_output.upper - bb_output.lower) / bb_output.average;
    }

    let signal = if last_rsi < config.rsi_low {
        RsiSignal::Buy
    } else if last_rsi > config.rsi_high {
        RsiSignal::Sell
    } else {
        RsiSignal::Hold
    };

    let volatility = if band_width > 0.10 {
        Volatility::High
    } else {
        Volatility::Low
    };

    Some(TechnicalReport {
        symbol: symbol.to_string(),
        price: last_price,
        rsi_14: last_rsi,

        volatility,
        signal,
    })
}

fn calculate_start_time() -> SystemTime {
    SystemTime::now() - std::time::Duration::from_secs(180 * 24 * 60 * 60)
}
