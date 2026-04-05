# Quasi Bots Workspace

## CI Status

[![CI](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml/badge.svg)](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml)

## Members

- **crypto**: Autonomous crypto trading worker (Binance spot + Polymarket predictions).
- **crypto-master**: Supervisor service that manages a fleet of crypto workers via gRPC.
- **communication**: Shared protobuf definitions and generated Rust types for gRPC.
- **skill-master**: Data collection and processing service (News, Knowledge, Finance).
- **finance**: Financial tracking and reporting logic.
- **monitor**: Monitoring service.
- **news**: News fetching and processing logic.
- **knowledge**: Spaced repetition system logic.
- **telemetry**: Metrics and logging utilities.

## Prerequisites

This workspace requires some system dependencies to be installed before you can use the services. The main dependencies are:
- **Protocol Buffers**: Used for serializing structured data.
- **SQLite**: Used for storing data in a lightweight database.
- **Fontconfig & Freetype**: Required for chart generation (SVG).

### Installation

**Ubuntu / Debian:**
```bash
sudo apt-get update
sudo apt-get install -y protobuf-compiler libsqlite3-dev pkg-config libfreetype6-dev libfontconfig1-dev
```

**Fedora:**
```bash
sudo dnf install protobuf-compiler sqlite-devel fontconfig-devel freetype-devel
```

**Arch Linux:**
```bash
sudo pacman -S protobuf sqlite fontconfig freetype2 pkgconf
```

**macOS:**
```bash
brew install protobuf fontconfig freetype
```

## Features

### Expense Tracking (Finance Module)

The system includes a comprehensive expense tracking module:

- **Entry Management**: Log daily expenses with categories.
- **Reporting**:
    - **Monthly**: Detailed breakdown by category for a specific month.
    - **Weekly**: Weekly summary of expenses.
    - **Yearly**: High-level view of annual spending trends.
- **Visualization**:
    - Generates SVG bar charts for reports.
    - Supports retrieving reports as JSON data or SVG images via API.
- **CLI Tool**: `report-cli` for generating charts from JSON report files offline.

### News Aggregation (News Module)

- **Feed Management**: Aggregates news from configured RSS feeds.
- **Daily Summary**: Provides a daily digest of important news items.
- **Topic Filtering**: Automatically categorizes news into topics.
- **Broken Link Detection**: Identifies and reports broken links in feeds.

### Knowledge Management (Knowledge Module)

- **Spaced Repetition**: Implements a spaced repetition system for efficient learning.
- **Daily Questions**: Serves daily questions for review based on affinity and schedule.
- **Review Tracking**: Tracks review attempts and success rates.
- **Tagging & Topics**: Organizes knowledge entries with tags and topics.

### Market Monitoring (Finance Module)

- **Portfolio Tracking**: Monitors asset portfolio performance.
- **Market Recommendations**: Provides market insights and recommendations.
- **Technical Analysis**: Performs technical analysis (e.g., RSI) on assets.

### Quotes Bank

- **Inspirational Quotes**: Stores and retrieves famous quotes.
- **Author Management**: Manages quote authors.

### API Documentation

The API is documented using Swagger/OpenAPI. You can access the Swagger UI when running `skill-master` (typically at `/scalar`).

The API includes endpoints for:
- **Expenses**: Categories, Entries, Reports (Monthly/Weekly/Yearly).
- **Market Tracker**: Portfolio, Market Recommendations.
- **Knowledge Bank**: Spaced repetition learning.
- **News Bank**: Aggregated news feeds.
- **Quotes Bank**: Inspirational quotes.

## Crypto Trading Bot

A master-worker architecture for autonomous crypto trading and Polymarket prediction.

### Architecture

```
crypto-cli --> crypto-master (gRPC :50050) --> worker-grok  (gRPC :50051)
                                           --> worker-gemini (gRPC :50051)
                                                    |
                                                PostgreSQL
```

- **Workers** (`crypto` binary) run two parallel loops: trading (Binance spot) and predictions (Polymarket CLOB). Each worker uses an LLM decision engine with heuristic fallback filters (RSI, Bollinger bands, edge detection, liquidity checks).
- **Master** (`crypto-master` binary) discovers workers from `WORKER_ADDRS`, proxies all RPCs, and computes performance metrics (P&L, win rate, Sharpe ratio, max drawdown, prediction accuracy).
- **CLI** (`crypto-cli` binary) talks to the master and provides subcommands for fleet management.

### Environment Variables

| Variable | Used by | Description |
|---|---|---|
| `DATABASE_URL` | worker | PostgreSQL connection string |
| `GRPC_ADDR` | worker | Worker gRPC listen address (default `0.0.0.0:50051`) |
| `WORKER_ID` | worker | Unique identifier for the worker instance |
| `LLM_PROVIDER` | worker | `grok` or `gemini` |
| `BINANCE_API_KEY` | worker | Binance API key |
| `BINANCE_SECRET_KEY` | worker | Binance HMAC secret |
| `POLYMARKET_API_KEY` | worker | Polymarket CLOB API key |
| `MASTER_GRPC_ADDR` | master | Master gRPC listen address (default `0.0.0.0:50050`) |
| `WORKER_ADDRS` | master | Comma-separated `id=url` pairs (e.g. `grok=http://worker-grok:50051`) |
| `MASTER_GRPC_URL` | CLI | Master endpoint (default `http://localhost:50050`) |
| `RUST_LOG` | all | Tracing filter (e.g. `info,crypto=debug`) |

### Running with Docker

```bash
# Set API keys in .env
echo "BINANCE_API_KEY=..." >> .env
echo "BINANCE_SECRET_KEY=..." >> .env
echo "POLYMARKET_API_KEY=..." >> .env

# Start the fleet
docker compose up -d postgres worker-grok worker-gemini master

# Use the CLI
docker compose run --rm master crypto-cli -e http://master:50050 workers
docker compose run --rm master crypto-cli -e http://master:50050 aggregate
```

### CLI Subcommands

```
workers              List connected workers
status <id>          Worker status
portfolio <id>       Worker portfolio
trades <id>          Open trading positions
predictions <id>     Open predictions
trade-history <id>   Closed trade records
prediction-history   Closed prediction records
performance <id>     Performance report (P&L, Sharpe, drawdown)
aggregate            Aggregate fleet performance
update-settings      Push new BotSettings to a worker
```

### Performance Tracking

The master computes per-worker and fleet-wide metrics from trade/prediction history:

- **P&L**: Total, realized, and unrealized profit/loss
- **Win rate**: Fraction of profitable trades
- **Sharpe ratio**: Risk-adjusted return
- **Max drawdown**: Worst peak-to-trough decline
- **Prediction accuracy**: Fraction of correct prediction outcomes

Use `crypto-cli aggregate` to compare workers side by side. The best-performing worker ID is highlighted in the aggregate report.

