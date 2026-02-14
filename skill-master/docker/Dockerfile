FROM rust:1.93-trixie AS planner

WORKDIR /build

RUN apt-get update && \
    apt-get install -y protobuf-compiler libsqlite3-dev && \
    rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef

COPY . .

RUN cargo chef prepare --bin skill-master --recipe-path recipe.json

FROM planner AS builder

WORKDIR /build

COPY --from=planner /build/recipe.json recipe.json

# Build dependencies separately to leverage Docker caching
RUN cargo chef cook --release --recipe-path recipe.json

COPY . . 

# Native build on the current platform
RUN cargo build --release --bin skill-master

FROM debian:trixie-slim AS runtime

WORKDIR /app

# The binary will be in the standard target/release directory
COPY --from=builder /build/target/release/skill-master /app/

ENV RUST_LOG=info

EXPOSE 8080

CMD ["/app/skill-master", "--config", "/config/config.toml"]
