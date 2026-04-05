use communication::proto;
use prost_types::Timestamp;

fn fmt_ts(ts: &Option<Timestamp>) -> String {
    match ts {
        Some(t) => {
            let secs = t.seconds;
            let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(
                secs,
                t.nanos as u32,
            );
            dt.map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| format!("{secs}s"))
        }
        None => "-".into(),
    }
}

pub fn print_worker_list(list: &proto::WorkerList) {
    if list.workers.is_empty() {
        println!("No workers connected.");
        return;
    }

    println!(
        "{:<15} {:<10} {:<8} {:<8} {:<8} {:<8} {:<22}",
        "WORKER", "LLM", "TRADING", "POLY", "TRADES", "PREDS", "HEARTBEAT"
    );
    println!("{}", "-".repeat(89));

    for w in &list.workers {
        println!(
            "{:<15} {:<10} {:<8} {:<8} {:<8} {:<8} {:<22}",
            w.worker_id,
            w.llm_provider,
            if w.trading_active { "active" } else { "off" },
            if w.polymarket_active { "active" } else { "off" },
            w.open_trade_count,
            w.open_prediction_count,
            fmt_ts(&w.last_heartbeat),
        );
    }
}

pub fn print_worker_status(s: &proto::WorkerStatus) {
    println!("Worker:       {}", s.worker_id);
    println!("LLM:          {}", s.llm_provider);
    println!(
        "Trading:      {}",
        if s.trading_active { "active" } else { "off" }
    );
    println!(
        "Polymarket:   {}",
        if s.polymarket_active { "active" } else { "off" }
    );
    println!("Open trades:  {}", s.open_trade_count);
    println!("Open preds:   {}", s.open_prediction_count);
    println!("Started:      {}", fmt_ts(&s.started_at));
    println!("Heartbeat:    {}", fmt_ts(&s.last_heartbeat));
}

pub fn print_portfolio(p: &proto::Portfolio) {
    println!(
        "Total balance:      {} {}",
        p.total_balance, p.base_currency
    );
    println!("Available:          {}", p.available_balance);
    println!("Trading allocated:  {}", p.trading_allocated);
    println!("Polymarket alloc:   {}", p.polymarket_allocated);
    println!("Unrealized P&L:     {}", p.unrealized_pnl);
    println!("Realized P&L:       {}", p.realized_pnl);
}

pub fn print_open_trades(resp: &proto::OpenTradesResponse) {
    if resp.positions.is_empty() {
        println!("No open positions.");
        return;
    }

    println!(
        "{:<12} {:<6} {:<14} {:<14} {:<14} {:<14} {:<14}",
        "PAIR", "SIDE", "ENTRY", "CURRENT", "QTY", "PNL", "STOP"
    );
    println!("{}", "-".repeat(94));

    for p in &resp.positions {
        println!(
            "{:<12} {:<6} {:<14} {:<14} {:<14} {:<14} {:<14}",
            p.pair,
            p.side,
            p.entry_price,
            p.current_price,
            p.quantity,
            p.unrealized_pnl,
            if p.stop_loss_price.is_empty() {
                "-"
            } else {
                &p.stop_loss_price
            },
        );
    }
}

pub fn print_open_predictions(resp: &proto::OpenPredictionsResponse) {
    if resp.predictions.is_empty() {
        println!("No open predictions.");
        return;
    }

    println!(
        "{:<36} {:<5} {:<12} {:<12} {:<12} {:<12}",
        "MARKET", "SIDE", "SHARES", "AVG", "CURRENT", "PNL"
    );
    println!("{}", "-".repeat(89));

    for p in &resp.predictions {
        let title = if p.market_title.len() > 34 {
            format!("{}...", &p.market_title[..31])
        } else {
            p.market_title.clone()
        };

        println!(
            "{:<36} {:<5} {:<12} {:<12} {:<12} {:<12}",
            title,
            p.side,
            p.shares,
            p.avg_price,
            p.current_price,
            p.unrealized_pnl,
        );
    }
}

pub fn print_trade_history(resp: &proto::TradeHistoryResponse) {
    if resp.records.is_empty() {
        println!("No trade records.");
        return;
    }

    println!(
        "{:<12} {:<5} {:<8} {:<14} {:<14} {:<14} {:<10} {:<22}",
        "PAIR", "SIDE", "TYPE", "QTY", "FILL_PRICE", "FEE", "STATUS", "CREATED"
    );
    println!("{}", "-".repeat(105));

    for r in &resp.records {
        println!(
            "{:<12} {:<5} {:<8} {:<14} {:<14} {:<14} {:<10} {:<22}",
            r.pair,
            r.side,
            r.order_type,
            r.filled_quantity,
            r.avg_fill_price,
            r.fee,
            r.status,
            fmt_ts(&r.created_at),
        );
    }
}

pub fn print_prediction_history(resp: &proto::PredictionHistoryResponse) {
    if resp.records.is_empty() {
        println!("No prediction records.");
        return;
    }

    println!(
        "{:<30} {:<5} {:<5} {:<12} {:<12} {:<12} {:<10} {:<10}",
        "MARKET", "SIDE", "ACT", "SHARES", "PRICE", "COST", "STATUS", "RESULT"
    );
    println!("{}", "-".repeat(100));

    for r in &resp.records {
        let title = if r.market_title.len() > 28 {
            format!("{}...", &r.market_title[..25])
        } else {
            r.market_title.clone()
        };

        println!(
            "{:<30} {:<5} {:<5} {:<12} {:<12} {:<12} {:<10} {:<10}",
            title,
            r.side,
            r.action,
            r.shares,
            r.price_per_share,
            r.total_cost,
            r.status,
            if r.resolution.is_empty() {
                "-"
            } else {
                &r.resolution
            },
        );
    }
}

pub fn print_performance(r: &proto::PerformanceReport) {
    println!("Worker:               {}", r.worker_id);
    println!("LLM provider:         {}", r.llm_provider);
    println!();
    println!("--- Trading ---");
    println!("Total P&L:            {}", r.total_pnl);
    println!("  Realized:           {}", r.realized_pnl);
    println!("  Unrealized:         {}", r.unrealized_pnl);
    println!("Win rate:             {}", r.win_rate);
    println!("Total trades:         {}", r.total_trades);
    println!("  Winning:            {}", r.winning_trades);
    println!("  Losing:             {}", r.losing_trades);
    println!("Sharpe ratio:         {}", r.sharpe_ratio);
    println!("Max drawdown:         {}", r.max_drawdown);
    println!();
    println!("--- Predictions ---");
    println!("Total predictions:    {}", r.total_predictions);
    println!("Correct:              {}", r.correct_predictions);
    println!("Accuracy:             {}", r.prediction_accuracy);
}

pub fn print_aggregate_performance(r: &proto::AggregatePerformanceReport) {
    println!("Fleet P&L:     {}", r.total_fleet_pnl);
    println!("Best worker:   {}", r.best_worker_id);
    println!();

    for report in &r.workers {
        println!("--- {} ({}) ---", report.worker_id, report.llm_provider);
        println!("  P&L:       {}", report.total_pnl);
        println!("  Win rate:  {}", report.win_rate);
        println!("  Sharpe:    {}", report.sharpe_ratio);
        println!("  Drawdown:  {}", report.max_drawdown);
        println!("  Pred acc:  {}", report.prediction_accuracy);
        println!();
    }
}

pub fn print_update_response(r: &proto::UpdateSettingsResponse) {
    if r.success {
        println!("Settings applied successfully.");
        println!("Message: {}", r.message);
        if let Some(s) = &r.applied_settings {
            println!();
            println!("Applied settings:");
            println!("  max_position_size_pct:    {}", s.max_position_size_pct);
            println!("  stop_loss_pct:            {}", s.stop_loss_pct);
            println!("  max_open_positions:       {}", s.max_open_positions);
            println!("  confidence_threshold:     {}", s.confidence_threshold);
            println!(
                "  trading_allocation_pct:   {}",
                s.trading_allocation_pct
            );
            println!(
                "  polymarket_allocation_pct:{}",
                s.polymarket_allocation_pct
            );
            println!("  llm_provider:             {}", s.llm_provider);
            println!("  llm_temperature:          {}", s.llm_temperature);
            println!("  allowed_pairs:            {:?}", s.allowed_pairs);
            println!("  trading_interval_secs:    {}", s.trading_interval_secs);
            println!(
                "  max_prediction_exposure:  {}",
                s.max_prediction_exposure
            );
            println!(
                "  min_liquidity_threshold:  {}",
                s.min_liquidity_threshold
            );
            println!(
                "  polymarket_interval_secs: {}",
                s.polymarket_interval_secs
            );
        }
    } else {
        eprintln!("Failed to apply settings: {}", r.message);
    }
}
