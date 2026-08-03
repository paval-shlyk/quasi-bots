# syntax=docker/dockerfile:1
FROM rust:1.93-trixie AS chef

WORKDIR /build

RUN apt-get update && \
    apt-get install -y protobuf-compiler libsqlite3-dev pkg-config libfreetype6-dev libfontconfig1-dev && \
    rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef

FROM chef AS planner

COPY . .

RUN cargo chef prepare --bin skill-master --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /build/recipe.json recipe.json

# Dependency layer: keyed on recipe.json. Cache mounts keep registry/git/target
# warm across rebuilds when the layer itself is invalidated.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo chef cook --release --recipe-path recipe.json

COPY . .

ARG GIT_COMMIT
# Binary must be copied out of the cache-mounted target/ into the image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    GIT_COMMIT=$GIT_COMMIT cargo build --release --bin skill-master && \
    cp target/release/skill-master /build/skill-master && \
    strip /build/skill-master

FROM debian:trixie-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates libfontconfig1 libfreetype6 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/skill-master /app/skill-master

ENV RUST_LOG=info

EXPOSE 8080

CMD ["/app/skill-master", "--config", "/config/config.toml"]
