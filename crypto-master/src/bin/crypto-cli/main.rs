use clap::{Parser, Subcommand};
use communication::proto;
use communication::MasterServiceClient;

mod display;
use display::*;


#[derive(Parser)]
#[command(name = "crypto-cli", about = "CLI for the crypto-master fleet manager")]
struct Cli {
    /// Master gRPC endpoint (e.g. http://localhost:50050)
    #[arg(short, long, env = "MASTER_GRPC_URL", default_value = "http://localhost:50050")]
    endpoint: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all connected workers
    Workers,

    /// Get a specific worker's status
    Status {
        /// Worker ID
        worker_id: String,
    },

    /// Get a worker's portfolio
    Portfolio {
        /// Worker ID
        worker_id: String,
    },

    /// Get a worker's open trading positions
    Trades {
        /// Worker ID
        worker_id: String,
    },

    /// Get a worker's open polymarket predictions
    Predictions {
        /// Worker ID
        worker_id: String,
    },

    /// Get a worker's trade history
    TradeHistory {
        /// Worker ID
        worker_id: String,
        /// Max records to return
        #[arg(short, long, default_value = "50")]
        limit: i32,
    },

    /// Get a worker's prediction history
    PredictionHistory {
        /// Worker ID
        worker_id: String,
        /// Max records to return
        #[arg(short, long, default_value = "50")]
        limit: i32,
    },

    /// Get a worker's performance report
    Performance {
        /// Worker ID
        worker_id: String,
    },

    /// Get aggregate performance across all workers
    Aggregate,

    /// Push new settings to a worker
    UpdateSettings {
        /// Worker ID
        worker_id: String,
        /// Settings as JSON string
        #[arg(long)]
        json: String,
    },
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut client = MasterServiceClient::connect(cli.endpoint).await?;

    match cli.command {
        Command::Workers => {
            let resp = client.list_workers(proto::Empty {}).await?.into_inner();
            print_worker_list(&resp);
        }
        Command::Status { worker_id } => {
            let resp = client
                .get_worker_status(proto::WorkerId { id: worker_id })
                .await?
                .into_inner();
            print_worker_status(&resp);
        }
        Command::Portfolio { worker_id } => {
            let resp = client
                .get_worker_portfolio(proto::WorkerId { id: worker_id })
                .await?
                .into_inner();
            print_portfolio(&resp);
        }
        Command::Trades { worker_id } => {
            let resp = client
                .get_worker_open_trades(proto::WorkerId { id: worker_id })
                .await?
                .into_inner();
            print_open_trades(&resp);
        }
        Command::Predictions { worker_id } => {
            let resp = client
                .get_worker_open_predictions(proto::WorkerId { id: worker_id })
                .await?
                .into_inner();
            print_open_predictions(&resp);
        }
        Command::TradeHistory { worker_id, limit } => {
            let resp = client
                .get_worker_trade_history(proto::WorkerHistoryRequest {
                    worker_id,
                    limit,
                    offset: 0,
                })
                .await?
                .into_inner();
            print_trade_history(&resp);
        }
        Command::PredictionHistory { worker_id, limit } => {
            let resp = client
                .get_worker_prediction_history(proto::WorkerHistoryRequest {
                    worker_id,
                    limit,
                    offset: 0,
                })
                .await?
                .into_inner();
            print_prediction_history(&resp);
        }
        Command::Performance { worker_id } => {
            let resp = client
                .get_performance_report(proto::WorkerId { id: worker_id })
                .await?
                .into_inner();
            print_performance(&resp);
        }
        Command::Aggregate => {
            let resp = client
                .get_aggregate_performance(proto::Empty {})
                .await?
                .into_inner();
            print_aggregate_performance(&resp);
        }
        Command::UpdateSettings { worker_id, json } => {
            let settings: SettingsInput = serde_json::from_str(&json)?;
            let proto_settings = settings.into_proto();

            let resp = client
                .update_worker_settings(proto::UpdateWorkerSettingsRequest {
                    worker_id,
                    settings: Some(proto_settings),
                })
                .await?
                .into_inner();
            print_update_response(&resp);
        }
    }

    Ok(())
}


/// Intermediate struct for JSON deserialization of settings from CLI input.
/// All fields are optional so the user can specify only what they want to change;
/// missing fields get sensible defaults that the worker will validate.
#[derive(serde::Deserialize)]
struct SettingsInput {
    #[serde(default = "default_position_size")]
    max_position_size_pct: String,
    #[serde(default = "default_stop_loss")]
    stop_loss_pct: String,
    #[serde(default = "default_max_open")]
    max_open_positions: i32,
    #[serde(default = "default_confidence")]
    confidence_threshold: String,
    #[serde(default = "default_trading_alloc")]
    trading_allocation_pct: String,
    #[serde(default = "default_poly_alloc")]
    polymarket_allocation_pct: String,
    #[serde(default = "default_llm_provider")]
    llm_provider: String,
    #[serde(default = "default_llm_temp")]
    llm_temperature: String,
    #[serde(default)]
    allowed_pairs: Vec<String>,
    #[serde(default = "default_trading_interval")]
    trading_interval_secs: u64,
    #[serde(default = "default_pred_exposure")]
    max_prediction_exposure: String,
    #[serde(default = "default_min_liquidity")]
    min_liquidity_threshold: String,
    #[serde(default = "default_poly_interval")]
    polymarket_interval_secs: u64,
}

fn default_position_size() -> String { "10".into() }
fn default_stop_loss() -> String { "5".into() }
fn default_max_open() -> i32 { 5 }
fn default_confidence() -> String { "0.7".into() }
fn default_trading_alloc() -> String { "50".into() }
fn default_poly_alloc() -> String { "50".into() }
fn default_llm_provider() -> String { "grok".into() }
fn default_llm_temp() -> String { "0.3".into() }
fn default_trading_interval() -> u64 { 60 }
fn default_pred_exposure() -> String { "100".into() }
fn default_min_liquidity() -> String { "50".into() }
fn default_poly_interval() -> u64 { 120 }

impl SettingsInput {
    fn into_proto(self) -> proto::BotSettings {
        proto::BotSettings {
            max_position_size_pct: self.max_position_size_pct,
            stop_loss_pct: self.stop_loss_pct,
            max_open_positions: self.max_open_positions,
            confidence_threshold: self.confidence_threshold,
            trading_allocation_pct: self.trading_allocation_pct,
            polymarket_allocation_pct: self.polymarket_allocation_pct,
            llm_provider: self.llm_provider,
            llm_temperature: self.llm_temperature,
            allowed_pairs: self.allowed_pairs,
            trading_interval_secs: self.trading_interval_secs,
            max_prediction_exposure: self.max_prediction_exposure,
            min_liquidity_threshold: self.min_liquidity_threshold,
            polymarket_interval_secs: self.polymarket_interval_secs,
        }
    }
}
