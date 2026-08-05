//! Technical indicators: pure compute (reusable by other crates) + thin data adapters.
//!
//! Network I/O stays in adapters; `compute_snapshot` only needs price closes.

use ta::Next;
use ta::indicators::{BollingerBands, RelativeStrengthIndex};

mod yahoo;

pub use yahoo::snapshot_from_yahoo;

#[derive(
    Debug,
    Clone,
    Copy,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub enum RsiSignal {
    Buy,
    Sell,
    Hold,
}

#[derive(
    Debug,
    Clone,
    Copy,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub enum Volatility {
    Low,
    High,
}

#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct TechnicalIndicators {
    pub price: f64,
    pub rsi_14: Option<f64>,
    pub signal: RsiSignal,
    pub volatility: Volatility,
}

#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub rsi_low: f64,
    pub rsi_high: f64,
    /// Bollinger band width above this → High volatility.
    pub high_volatility_band_width: f64,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            rsi_low: 30.0,
            rsi_high: 70.0,
            high_volatility_band_width: 0.10,
        }
    }
}

/// Single OHLCV bar (exchange-agnostic).
#[derive(Debug, Clone, Copy)]
pub struct Candle {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Build a technical snapshot from chronological close prices (oldest → newest).
pub fn compute_snapshot(
    closes: &[f64],
    config: &AnalysisConfig,
) -> Option<TechnicalIndicators> {
    if closes.is_empty() {
        return None;
    }

    let price = *closes.last()?;

    let mut rsi_ind = RelativeStrengthIndex::new(14).ok()?;
    let mut bb_ind = BollingerBands::new(20, 2.0).ok()?;

    let mut last_rsi = 0.0;
    let mut band_width = 0.0;
    let mut rsi_ready = false;
    let mut bb_ready = false;

    for (i, &close) in closes.iter().enumerate() {
        last_rsi = rsi_ind.next(close);
        if i + 1 >= 15 {
            rsi_ready = true;
        }
        let bb = bb_ind.next(close);
        if i + 1 >= 20 {
            bb_ready = true;
            if bb.average > 0.0 {
                band_width = (bb.upper - bb.lower) / bb.average;
            }
        }
    }

    let rsi_14 = rsi_ready.then_some(last_rsi);
    let signal = match rsi_14 {
        Some(r) if r < config.rsi_low => RsiSignal::Buy,
        Some(r) if r > config.rsi_high => RsiSignal::Sell,
        _ => RsiSignal::Hold,
    };

    let volatility =
        if bb_ready && band_width > config.high_volatility_band_width {
            Volatility::High
        } else {
            Volatility::Low
        };

    Some(TechnicalIndicators {
        price,
        rsi_14,
        signal,
        volatility,
    })
}

/// Convenience: closes from candle closes.
pub fn compute_snapshot_from_candles(
    candles: &[Candle],
    config: &AnalysisConfig,
) -> Option<TechnicalIndicators> {
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    compute_snapshot(&closes, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rising_closes(n: usize) -> Vec<f64> {
        (0..n).map(|i| 100.0 + i as f64).collect()
    }

    #[test]
    fn given_empty_closes_when_compute_snapshot_then_returns_none() {
        // Arrange
        let closes: &[f64] = &[];
        let config = AnalysisConfig::default();

        // Act
        let snap = compute_snapshot(closes, &config);

        // Assert
        assert!(snap.is_none());
    }

    #[test]
    fn given_enough_rising_closes_when_compute_snapshot_then_has_price_and_rsi()
    {
        // Arrange: strong uptrend → RSI tends high
        let closes = rising_closes(40);
        let config = AnalysisConfig::default();

        // Act
        let snap = compute_snapshot(&closes, &config).expect("snapshot");

        // Assert
        assert!((snap.price - 139.0).abs() < 1e-9);
        assert!(snap.rsi_14.is_some());
        let rsi = snap.rsi_14.unwrap();
        assert!(
            rsi > 50.0,
            "rising series should have elevated RSI, got {rsi}"
        );
    }

    #[test]
    fn given_flat_closes_when_compute_snapshot_then_signal_is_hold() {
        // Arrange
        let closes = vec![100.0; 40];
        let config = AnalysisConfig::default();

        // Act
        let snap = compute_snapshot(&closes, &config).expect("snapshot");

        // Assert
        assert!(matches!(snap.signal, RsiSignal::Hold));
        assert!((snap.price - 100.0).abs() < 1e-9);
    }
}
