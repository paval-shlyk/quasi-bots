use clap::{Parser, Subcommand};
use dotenvy::dotenv;
use std::env;

/// Small CLI for exercising REST and WebSocket endpoints in the finance crate.
#[derive(Parser)]
#[command(name = "finance-cli")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Call REST /time endpoint
    Time {
        url: String,
    },
    /// Call REST /depth endpoint for symbol
    Depth {
        url: String,
        symbol: String,
    },
    /// Call REST /exchangeInfo endpoint
    ExchangeInfo {
        url: String,
    },
    /// Call REST /currencies endpoint
    Currencies {
        url: String,
    },
    /// Call REST /klines endpoint
    Klines {
        url: String,
        symbol: String,
        interval: String,
    },
    /// Call REST /account endpoint (requires API_KEY/API_SECRET env)
    Account {
        url: String,
    },
    /// Call REST /deposits endpoint (requires API_KEY/API_SECRET env)
    Deposits {
        url: String,
    },
    /// Call REST /myTrades endpoint (requires API_KEY/API_SECRET env)
    MyTrades {
        url: String,
        symbol: String,
    },
    /// Call REST /fetchOrder endpoint (requires API_KEY/API_SECRET env)
    FetchOrder {
        url: String,
        order_id: String,
    },
    /// Call REST /openOrders endpoint (requires API_KEY/API_SECRET env)
    OpenOrders {
        url: String,
        symbol: Option<String>,
    },
    /// Call REST /ledger endpoint (requires API_KEY/API_SECRET env)
    Ledger {
        url: String,
        currency: Option<String>,
    },
    /// Call REST /transactions endpoint (requires API_KEY/API_SECRET env)
    Transactions {
        url: String,
    },
    /// Call REST /tradingPositions endpoint (requires API_KEY/API_SECRET env)
    TradingPositions {
        url: String,
    },
    /// Call REST /tradingPositionHistory endpoint (requires API_KEY/API_SECRET env)
    TradingPositionHistory {
        url: String,
        symbol: Option<String>,
    },
    Ticker {
        url: String,
        symbol: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // load .env if present
    let _ = dotenv();

    match cli.cmd {
        Commands::Time { url } => {
            let api_key = env::var("API_KEY").unwrap_or_default();
            let api_secret = env::var("API_SECRET").unwrap_or_default();
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let st = rc.time().await?;
            println!("serverTime: {}", st);
        }
        Commands::Depth { url, symbol } => {
            let api_key = env::var("API_KEY").unwrap_or_default();
            let api_secret = env::var("API_SECRET").unwrap_or_default();
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let v = rc.depth(&symbol).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::ExchangeInfo { url } => {
            let api_key = env::var("API_KEY").unwrap_or_default();
            let api_secret = env::var("API_SECRET").unwrap_or_default();
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);

            let ts = rc.time().await?;
            let v = rc.exchange_info(ts).await?;

            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Account { url } => {
            let api_key =
                env::var("API_KEY").expect("API_KEY required for account");
            let api_secret = env::var("API_SECRET")
                .expect("API_SECRET required for account");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let v = rc.account(ts).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Currencies { url } => {
            let api_key = env::var("API_KEY").unwrap_or_default();
            let api_secret = env::var("API_SECRET").unwrap_or_default();
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);

            let ts = rc.time().await?;
            let v = rc.currencies(ts).await?;

            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Klines {
            url,
            symbol,
            interval,
        } => {
            let api_key = env::var("API_KEY").unwrap_or_default();
            let api_secret = env::var("API_SECRET").unwrap_or_default();
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let v = rc.klines(&symbol, &interval).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Deposits { url } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let v = rc.deposits(ts).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::MyTrades { url, symbol } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let v = rc.my_trades(&symbol, ts).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::FetchOrder { url, order_id } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let v = rc.fetch_order(&order_id, ts).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::OpenOrders { url, symbol } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let v = rc.open_orders(symbol.as_deref(), ts).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Ledger { url, currency } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let mut v = rc.fetch_full_ledger(currency.as_deref(), ts).await?;
            // .into_iter()
            // .filter(|e| !matches!(e.ty, finance::LedgerEntryType::Deposit))
            // .collect::<Vec<_>>();

            v.sort_by_key(|e| -e.timestamp.timestamp_millis());
            println!("{:#?}", v);
            println!("Total count = {}", v.len());
        }
        Commands::Transactions { url } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let v = rc.fetch_all_transactions(ts).await?;
            println!("{:#?}", v);
            println!("Total count = {}", v.len());
        }
        Commands::TradingPositions { url } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let v = rc.trading_positions(ts).await?;
            println!("{:#?}", v);
        }
        Commands::TradingPositionHistory { url, symbol } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);
            let ts = rc.time().await?;
            let v = rc.trading_position_history(symbol.as_deref(), ts).await?;
            println!("{:#?}", v);
        }
        Commands::Ticker { url, symbol } => {
            let api_key = env::var("API_KEY").expect("API_KEY required");
            let api_secret =
                env::var("API_SECRET").expect("API_SECRET required");
            let rc =
                finance::portfolio::RestClient::new(url, api_key, api_secret);

            let v = rc.ticker(&symbol).await?;

            println!("{:#?}", v);
        }
    }

    Ok(())
}
