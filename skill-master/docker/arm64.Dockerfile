FROM rust:1.93-trixie AS builder

WORKDIR /build

RUN apt-get update && \
    apt-get install -y \
    protobuf-compiler libsqlite3-dev \
    gcc-aarch64-linux-gnu \
    g++-aarch64-linux-gnu \
    binutils-aarch64-linux-gnu \
    && \
    rm -rf /var/lib/apt/lists/*

COPY . .

RUN rustup target add aarch64-unknown-linux-gnu

ENV CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
ENV CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

RUN cargo build --release -p skill-master --target aarch64-unknown-linux-gnu

FROM debian:stable-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libsqlite3-0 \
    && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/target/aarch64-unknown-linux-gnu/release/skill-master /app/

ENV RUST_LOG=info

EXPOSE 8080

CMD ["/app/skill-master", "--config", "/config/config.toml"]
