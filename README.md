# Quasi Bots Workspace

## CI Status

[![CI](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml/badge.svg)](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml)

## Members

- **scrapper**: Data collection and processing service.
- **monitor**: Monitoring service.

## Prerequisites

This workspace requires some system dependencies to be installed before you can use the services. The main dependencies are:
- **Protocol Buffers**: Used for serializing structured data.
- **SQLite**: Used for storing data in a lightweight database.

### Installation

**Ubuntu / Debian:**
```bash
sudo apt-get update
sudo apt-get install -y protobuf-compiler libsqlite3-dev
```

**Fedora:**
```bash
sudo dnf install protobuf-compiler sqlite-devel
```

**Arch Linux:**
```bash
sudo pacman -S protobuf sqlite
```

**macOS:**
```bash
brew install protobuf
```
