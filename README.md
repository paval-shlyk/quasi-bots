# Quasi Bots Workspace

## CI Status

[![CI](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml/badge.svg)](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml)

## Members

- **skill-master**: Data collection service exposed as an MCP server (quotes, knowledge, expenses, trading, search).
- **finance**: Financial tracking and reporting logic used by skill-master.
- **knowledge**: Spaced repetition system logic.
- **news**: News fetching and processing logic.
- **telemetry**: Metrics and logging utilities.
- **mcp-auth**: OAuth 2.1 authorization server library for MCP resource servers.
- **mcp-client**: MCP Streamable HTTP client with a TUI for verifying servers.
- **crypto**: Autonomous crypto trading worker (Binance spot + Polymarket) — **under development, not tested yet**.
- **crypto-master**: Supervisor service that manages a fleet of crypto workers via gRPC — **under development**.
- **communication**: Shared protobuf definitions and generated Rust types for gRPC.

## Prerequisites

This workspace requires some system dependencies before you can build and run the services:

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

## skill-master (MCP server)

The primary user-facing surface today is **skill-master** as an **MCP server** (Streamable HTTP, typically at `/mcp`). There is no public REST/HTTP feature API for clients; tools are invoked over MCP.

### Tools

| Area | Capability |
|------|------------|
| **Quotes** | List authors; fetch the next unused famous quote. |
| **Knowledge** | Spaced-repetition bank: next question, topics/tags, add entries, record reviews, set affinity. |
| **Expenses** | Categories and entries; create/update/delete; monthly/weekly/yearly reports. |
| **Trading** | Limited portfolio summary (not full bot control). |
| **Search** | Web search via SerpAPI (Google). |

Auth is handled by the nested OAuth flow (`mcp-auth`); see [mcp-auth/README.md](mcp-auth/README.md) and [mcp-client/README.md](mcp-client/README.md) for connecting a client.

### Domain modules (libraries)

These power the MCP tools above:

- **Expenses**: entry management, category breakdowns, SVG charts (also usable offline via `report-cli`).
- **Knowledge**: spaced repetition, daily questions, review tracking, tags/topics.
- **Quotes**: inspirational quotes and authors.
- **Trading / market helpers**: portfolio-oriented helpers used by the limited trading tool.

## CI pipeline

Workflow: [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

Release **version** is `{major.minor from skill-master/Cargo.toml}.{github.run_number}` (optional manual prefix override on `workflow_dispatch`).

Two channels publish separate GitHub Releases, GHCR images, and `.deb` packages — install only one per host:

- **stable** (`main`): package/image `skill-master`, release tag `{version}`
- **dev**: package/image `skill-master-dev`, prerelease tag `dev-{version}`

## Crypto trading bots (under development)

Master–worker crypto trading (Binance spot + Polymarket) and the related CLI are **not production-ready**: functionality is incomplete and **not covered by CI/tests yet**. Treat the `crypto` / `crypto-master` crates as experimental.

High-level shape (subject to change):

```
crypto-cli --> crypto-master (gRPC) --> workers (gRPC)
                                         |
                                     PostgreSQL
```

Do not rely on the env vars, Docker Compose layout, or CLI subcommands for real trading until this area is tested and documented again.
