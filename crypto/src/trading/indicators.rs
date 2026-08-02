use rust_decimal::Decimal;
use ta::Next;
use ta::indicators::{
    BollingerBands as TaBollinger, ExponentialMovingAverage,
    MovingAverageConvergenceDivergence, RelativeStrengthIndex,
    SimpleMovingAverage,
};

use super::models::{BollingerBands, Candle, MACDValues, TechnicalIndicators};

/// Computes all technical indicators from a slice of candles.
/// Requires at least 26 candles for MACD/EMA-26 to produce meaningful values.
pub fn compute_indicators(candles: &[Candle]) -> TechnicalIndicators {
    if candles.is_empty() {
        return TechnicalIndicators {
            rsi: None,
            macd: None,
            bollinger: None,
            sma_20: None,
            ema_12: None,
            ema_26: None,
        };
    }

    let rsi = compute_rsi(candles, 14);
    let macd = compute_macd(candles, 12, 26, 9);
    let bollinger = compute_bollinger(candles, 20, 2.0);
    let sma_20 = compute_sma(candles, 20);
    let ema_12 = compute_ema(candles, 12);
    let ema_26 = compute_ema(candles, 26);

    TechnicalIndicators {
        rsi,
        macd,
        bollinger,
        sma_20,
        ema_12,
        ema_26,
    }
}

fn to_f64(d: Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

fn to_decimal(f: f64) -> Decimal {
    Decimal::try_from(f).unwrap_or(Decimal::ZERO)
}

fn compute_rsi(candles: &[Candle], period: usize) -> Option<Decimal> {
    if candles.len() < period + 1 {
        return None;
    }

    let mut rsi = RelativeStrengthIndex::new(period).ok()?;
    let mut last = 0.0;
    for candle in candles {
        last = rsi.next(to_f64(candle.close));
    }
    Some(to_decimal(last))
}

fn compute_macd(
    candles: &[Candle],
    fast: usize,
    slow: usize,
    signal: usize,
) -> Option<MACDValues> {
    if candles.len() < slow + signal {
        return None;
    }

    let mut macd =
        MovingAverageConvergenceDivergence::new(fast, slow, signal).ok()?;
    let mut last = ta::indicators::MovingAverageConvergenceDivergenceOutput {
        macd: 0.0,
        signal: 0.0,
        histogram: 0.0,
    };
    for candle in candles {
        last = macd.next(to_f64(candle.close));
    }
    Some(MACDValues {
        macd_line: to_decimal(last.macd),
        signal_line: to_decimal(last.signal),
        histogram: to_decimal(last.histogram),
    })
}

fn compute_bollinger(
    candles: &[Candle],
    period: usize,
    multiplier: f64,
) -> Option<BollingerBands> {
    if candles.len() < period {
        return None;
    }

    let mut bb = TaBollinger::new(period, multiplier).ok()?;
    let mut last = ta::indicators::BollingerBandsOutput {
        average: 0.0,
        upper: 0.0,
        lower: 0.0,
    };
    for candle in candles {
        last = bb.next(to_f64(candle.close));
    }
    Some(BollingerBands {
        upper: to_decimal(last.upper),
        middle: to_decimal(last.average),
        lower: to_decimal(last.lower),
    })
}

fn compute_sma(candles: &[Candle], period: usize) -> Option<Decimal> {
    if candles.len() < period {
        return None;
    }

    let mut sma = SimpleMovingAverage::new(period).ok()?;
    let mut last = 0.0;
    for candle in candles {
        last = sma.next(to_f64(candle.close));
    }
    Some(to_decimal(last))
}

fn compute_ema(candles: &[Candle], period: usize) -> Option<Decimal> {
    if candles.len() < period {
        return None;
    }

    let mut ema = ExponentialMovingAverage::new(period).ok()?;
    let mut last = 0.0;
    for candle in candles {
        last = ema.next(to_f64(candle.close));
    }
    Some(to_decimal(last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_candles(prices: &[f64]) -> Vec<Candle> {
        prices
            .iter()
            .map(|&p| {
                let d = Decimal::try_from(p).unwrap();
                Candle {
                    open: d,
                    high: d + Decimal::ONE,
                    low: d - Decimal::ONE,
                    close: d,
                    volume: Decimal::new(1000, 0),
                    timestamp: Utc::now(),
                }
            })
            .collect()
    }

    #[test]
    fn rsi_with_enough_candles() {
        let prices: Vec<f64> =
            (0..30).map(|i| 100.0 + (i as f64) * 0.5).collect();
        let candles = make_candles(&prices);
        let rsi = compute_rsi(&candles, 14);
        assert!(rsi.is_some());
        let val = rsi.unwrap();
        // Monotonically increasing prices should push RSI high
        assert!(
            val > Decimal::new(60, 0),
            "RSI should be > 60 for uptrend, got {val}"
        );
    }

    #[test]
    fn rsi_with_too_few_candles() {
        let candles = make_candles(&[100.0; 10]);
        assert!(compute_rsi(&candles, 14).is_none());
    }

    #[test]
    fn bollinger_bands_produce_sane_values() {
        let prices: Vec<f64> =
            (0..25).map(|i| 50.0 + (i as f64 % 5.0)).collect();
        let candles = make_candles(&prices);
        let bb = compute_bollinger(&candles, 20, 2.0);
        assert!(bb.is_some());
        let bb = bb.unwrap();
        assert!(bb.upper > bb.middle);
        assert!(bb.middle > bb.lower);
    }

    #[test]
    fn macd_with_enough_candles() {
        let prices: Vec<f64> =
            (0..40).map(|i| 100.0 + (i as f64).sin() * 5.0).collect();
        let candles = make_candles(&prices);
        let macd = compute_macd(&candles, 12, 26, 9);
        assert!(macd.is_some());
    }

    #[test]
    fn compute_indicators_empty() {
        let ti = compute_indicators(&[]);
        assert!(ti.rsi.is_none());
        assert!(ti.macd.is_none());
        assert!(ti.bollinger.is_none());
    }

    #[test]
    fn full_indicators_from_sufficient_data() {
        let prices: Vec<f64> =
            (0..50).map(|i| 200.0 + (i as f64) * 0.3).collect();
        let candles = make_candles(&prices);
        let ti = compute_indicators(&candles);
        assert!(ti.rsi.is_some());
        assert!(ti.macd.is_some());
        assert!(ti.bollinger.is_some());
        assert!(ti.sma_20.is_some());
        assert!(ti.ema_12.is_some());
        assert!(ti.ema_26.is_some());
    }
}
