use communication::proto;
use rust_decimal::Decimal;

/// Compute performance metrics from a worker's trade and prediction history.
pub fn compute_performance(
    worker_id: &str,
    llm_provider: &str,
    portfolio: &proto::Portfolio,
    trades: &[proto::TradeRecord],
    predictions: &[proto::PredictionRecord],
) -> proto::PerformanceReport {
    let (winning, losing) = count_trade_outcomes(trades);
    let total_trades = trades.len() as i32;
    let win_rate = if total_trades > 0 {
        Decimal::from(winning) / Decimal::from(total_trades)
    } else {
        Decimal::ZERO
    };

    let realized: Decimal =
        portfolio.realized_pnl.parse().unwrap_or(Decimal::ZERO);
    let unrealized: Decimal =
        portfolio.unrealized_pnl.parse().unwrap_or(Decimal::ZERO);
    let total_pnl = realized + unrealized;

    let sharpe = compute_sharpe(trades);
    let max_dd = compute_max_drawdown(trades);

    let (correct, total_pred) = count_prediction_outcomes(predictions);
    let pred_accuracy = if total_pred > 0 {
        Decimal::from(correct) / Decimal::from(total_pred)
    } else {
        Decimal::ZERO
    };

    proto::PerformanceReport {
        worker_id: worker_id.into(),
        llm_provider: llm_provider.into(),
        total_pnl: total_pnl.to_string(),
        realized_pnl: realized.to_string(),
        unrealized_pnl: unrealized.to_string(),
        win_rate: win_rate.to_string(),
        total_trades,
        winning_trades: winning,
        losing_trades: losing,
        sharpe_ratio: sharpe.to_string(),
        max_drawdown: max_dd.to_string(),
        total_predictions: total_pred,
        correct_predictions: correct,
        prediction_accuracy: pred_accuracy.to_string(),
        period_start: None,
        period_end: None,
    }
}

fn count_trade_outcomes(trades: &[proto::TradeRecord]) -> (i32, i32) {
    let mut winning = 0i32;
    let mut losing = 0i32;

    for trade in trades {
        if trade.status != "filled" {
            continue;
        }
        let fill: Decimal =
            trade.avg_fill_price.parse().unwrap_or(Decimal::ZERO);
        let entry: Decimal = trade.price.parse().unwrap_or(Decimal::ZERO);
        let fee: Decimal = trade.fee.parse().unwrap_or(Decimal::ZERO);

        let pnl = match trade.side.as_str() {
            "sell" => fill - entry - fee,
            _ => continue, // only evaluate closed trades (sells)
        };

        if pnl > Decimal::ZERO {
            winning += 1;
        } else if pnl < Decimal::ZERO {
            losing += 1;
        }
    }

    (winning, losing)
}

/// Simplified Sharpe ratio: mean(returns) / stddev(returns).
/// Returns 0 if insufficient data (< 2 sell trades).
fn compute_sharpe(trades: &[proto::TradeRecord]) -> Decimal {
    let returns: Vec<Decimal> = trades
        .iter()
        .filter(|t| t.status == "filled" && t.side == "sell")
        .filter_map(|t| {
            let fill: Decimal = t.avg_fill_price.parse().ok()?;
            let entry: Decimal = t.price.parse().ok()?;
            let fee: Decimal = t.fee.parse().ok()?;
            if entry == Decimal::ZERO {
                return None;
            }
            Some((fill - entry - fee) / entry)
        })
        .collect();

    if returns.len() < 2 {
        return Decimal::ZERO;
    }

    let n = Decimal::from(returns.len() as i64);
    let mean = returns.iter().sum::<Decimal>() / n;
    let variance = returns
        .iter()
        .map(|r| {
            let diff = *r - mean;
            diff * diff
        })
        .sum::<Decimal>()
        / (n - Decimal::ONE);

    if variance <= Decimal::ZERO {
        return Decimal::ZERO;
    }

    // Integer sqrt approximation via Newton's method on Decimal
    let stddev = decimal_sqrt(variance);
    if stddev == Decimal::ZERO {
        return Decimal::ZERO;
    }

    mean / stddev
}

/// Approximate max drawdown from cumulative P&L of sell trades.
fn compute_max_drawdown(trades: &[proto::TradeRecord]) -> Decimal {
    let mut cumulative = Decimal::ZERO;
    let mut peak = Decimal::ZERO;
    let mut max_dd = Decimal::ZERO;

    for trade in trades {
        if trade.status != "filled" || trade.side != "sell" {
            continue;
        }
        let fill: Decimal =
            trade.avg_fill_price.parse().unwrap_or(Decimal::ZERO);
        let entry: Decimal = trade.price.parse().unwrap_or(Decimal::ZERO);
        let fee: Decimal = trade.fee.parse().unwrap_or(Decimal::ZERO);
        let qty: Decimal =
            trade.filled_quantity.parse().unwrap_or(Decimal::ONE);

        cumulative += (fill - entry) * qty - fee;
        if cumulative > peak {
            peak = cumulative;
        }
        let dd = peak - cumulative;
        if dd > max_dd {
            max_dd = dd;
        }
    }

    max_dd
}

fn count_prediction_outcomes(
    predictions: &[proto::PredictionRecord],
) -> (i32, i32) {
    let mut correct = 0i32;
    let mut total = 0i32;

    for pred in predictions {
        match pred.resolution.as_str() {
            "won" => {
                correct += 1;
                total += 1;
            }
            "lost" => {
                total += 1;
            }
            _ => {} // unresolved
        }
    }

    (correct, total)
}

/// Newton's method square root for Decimal (10 iterations).
fn decimal_sqrt(val: Decimal) -> Decimal {
    if val <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let mut guess = val / Decimal::TWO;
    for _ in 0..10 {
        if guess == Decimal::ZERO {
            return Decimal::ZERO;
        }
        guess = (guess + val / guess) / Decimal::TWO;
    }
    guess
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trade(
        side: &str,
        status: &str,
        price: &str,
        avg_fill: &str,
        fee: &str,
        filled_qty: &str,
    ) -> proto::TradeRecord {
        proto::TradeRecord {
            id: "t-1".into(),
            pair: "BTC/USDC".into(),
            side: side.into(),
            order_type: "limit".into(),
            quantity: filled_qty.into(),
            price: price.into(),
            filled_quantity: filled_qty.into(),
            avg_fill_price: avg_fill.into(),
            fee: fee.into(),
            status: status.into(),
            llm_rationale: String::new(),
            llm_confidence: "0.8".into(),
            created_at: None,
            updated_at: None,
        }
    }

    fn make_prediction(resolution: &str) -> proto::PredictionRecord {
        proto::PredictionRecord {
            id: "p-1".into(),
            market_id: "mkt-1".into(),
            market_title: "Test".into(),
            side: "yes".into(),
            action: "buy".into(),
            shares: "10".into(),
            price_per_share: "0.60".into(),
            total_cost: "6.00".into(),
            status: "filled".into(),
            resolution: resolution.into(),
            llm_rationale: String::new(),
            llm_confidence: "0.75".into(),
            created_at: None,
            updated_at: None,
        }
    }

    fn default_portfolio() -> proto::Portfolio {
        proto::Portfolio {
            total_balance: "10000".into(),
            available_balance: "8000".into(),
            trading_allocated: "1500".into(),
            polymarket_allocated: "500".into(),
            unrealized_pnl: "50".into(),
            realized_pnl: "200".into(),
            base_currency: "USDC".into(),
        }
    }

    #[test]
    fn count_outcomes_only_counts_filled_sells() {
        let trades = vec![
            make_trade("sell", "filled", "100", "110", "1", "1"), // win: 110-100-1=9 > 0
            make_trade("sell", "filled", "100", "90", "1", "1"), // loss: 90-100-1=-11 < 0
            make_trade("buy", "filled", "100", "110", "1", "1"), // buy, ignored
            make_trade("sell", "pending", "100", "110", "1", "1"), // not filled, ignored
        ];
        let (w, l) = count_trade_outcomes(&trades);
        assert_eq!(w, 1);
        assert_eq!(l, 1);
    }

    #[test]
    fn count_outcomes_empty() {
        let (w, l) = count_trade_outcomes(&[]);
        assert_eq!(w, 0);
        assert_eq!(l, 0);
    }

    #[test]
    fn count_prediction_outcomes_mixed() {
        let preds = vec![
            make_prediction("won"),
            make_prediction("won"),
            make_prediction("lost"),
            make_prediction("pending"), // unresolved, ignored
        ];
        let (correct, total) = count_prediction_outcomes(&preds);
        assert_eq!(correct, 2);
        assert_eq!(total, 3);
    }

    #[test]
    fn max_drawdown_basic() {
        // Three winning sells then one big loss
        let trades = vec![
            make_trade("sell", "filled", "100", "110", "0", "1"), // pnl +10
            make_trade("sell", "filled", "100", "120", "0", "1"), // pnl +20 (cum=30, peak=30)
            make_trade("sell", "filled", "100", "70", "0", "1"), // pnl -30 (cum=0, dd=30)
        ];
        let dd = compute_max_drawdown(&trades);
        assert_eq!(dd, Decimal::new(30, 0));
    }

    #[test]
    fn max_drawdown_no_trades() {
        assert_eq!(compute_max_drawdown(&[]), Decimal::ZERO);
    }

    #[test]
    fn sharpe_insufficient_data() {
        let trades = vec![make_trade("sell", "filled", "100", "110", "0", "1")];
        assert_eq!(compute_sharpe(&trades), Decimal::ZERO);
    }

    #[test]
    fn sharpe_with_data() {
        // Two sell trades with different returns
        let trades = vec![
            make_trade("sell", "filled", "100", "120", "0", "1"), // return = 20/100 = 0.2
            make_trade("sell", "filled", "100", "110", "0", "1"), // return = 10/100 = 0.1
            make_trade("sell", "filled", "100", "130", "0", "1"), // return = 30/100 = 0.3
        ];
        let s = compute_sharpe(&trades);
        // mean = 0.2, stddev = 0.1, sharpe = 2.0
        assert!(s > Decimal::ONE, "Sharpe should be positive, got {s}");
    }

    #[test]
    fn compute_performance_end_to_end() {
        let portfolio = default_portfolio();
        let trades = vec![
            make_trade("sell", "filled", "100", "120", "1", "1"),
            make_trade("sell", "filled", "100", "80", "1", "1"),
        ];
        let predictions = vec![make_prediction("won"), make_prediction("lost")];

        let report = compute_performance(
            "worker-1",
            "grok",
            &portfolio,
            &trades,
            &predictions,
        );

        assert_eq!(report.worker_id, "worker-1");
        assert_eq!(report.llm_provider, "grok");
        assert_eq!(report.total_trades, 2);
        assert_eq!(report.winning_trades, 1);
        assert_eq!(report.losing_trades, 1);
        // total_pnl = realized(200) + unrealized(50) = 250
        assert_eq!(report.total_pnl, "250");
        assert_eq!(report.realized_pnl, "200");
        assert_eq!(report.unrealized_pnl, "50");
        assert_eq!(report.total_predictions, 2);
        assert_eq!(report.correct_predictions, 1);
        assert_eq!(report.prediction_accuracy, "0.50");
        assert_eq!(report.win_rate, "0.50");
    }

    #[test]
    fn compute_performance_no_data() {
        let portfolio = proto::Portfolio {
            total_balance: "0".into(),
            available_balance: "0".into(),
            trading_allocated: "0".into(),
            polymarket_allocated: "0".into(),
            unrealized_pnl: "0".into(),
            realized_pnl: "0".into(),
            base_currency: "USDC".into(),
        };

        let report = compute_performance("w-2", "gemini", &portfolio, &[], &[]);

        assert_eq!(report.total_trades, 0);
        assert_eq!(report.winning_trades, 0);
        assert_eq!(report.losing_trades, 0);
        assert_eq!(report.total_pnl, "0");
        assert_eq!(report.win_rate, "0");
        assert_eq!(report.sharpe_ratio, "0");
        assert_eq!(report.max_drawdown, "0");
        assert_eq!(report.total_predictions, 0);
        assert_eq!(report.prediction_accuracy, "0");
    }

    #[test]
    fn decimal_sqrt_basic() {
        let result = decimal_sqrt(Decimal::new(4, 0));
        // Should be very close to 2.0
        let diff = (result - Decimal::TWO).abs();
        assert!(
            diff < Decimal::new(1, 10),
            "sqrt(4) should be ~2.0, got {result}"
        );
    }

    #[test]
    fn decimal_sqrt_zero_and_negative() {
        assert_eq!(decimal_sqrt(Decimal::ZERO), Decimal::ZERO);
        assert_eq!(decimal_sqrt(Decimal::new(-5, 0)), Decimal::ZERO);
    }
}
