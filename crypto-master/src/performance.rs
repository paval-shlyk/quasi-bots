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
