# Quasi Bots Workspace

## CI Status

[![CI](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml/badge.svg)](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml)

## Members

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

