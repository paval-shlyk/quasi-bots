# Quasi Bots Workspace

## CI Status

[![CI](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml/badge.svg)](https://github.com/paval-shlyk/quasi-bots/actions/workflows/ci.yml)

## Members

- **skill-master**: Data collection and processing service.
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

### Cross-compilation Toolchain (AArch64)

To cross-compile for ARM64, you need the GNU toolchain:

**Ubuntu / Debian:**
```bash
sudo apt-get install gcc-aarch64-linux-gnu
```

**Fedora:**
```bash
sudo dnf install gcc-aarch64-linux-gnu sysroot-aarch64-fc42-glibc
```

**Arch Linux:**
```bash
sudo pacman -S aarch64-linux-gnu-gcc
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
