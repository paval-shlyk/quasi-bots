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

# Build dependencies - this layer is cached until Cargo.toml changes
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

ARG GIT_COMMIT
RUN GIT_COMMIT=$GIT_COMMIT cargo build --release --bin skill-master && \
    strip target/release/skill-master

FROM debian:trixie-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/target/release/skill-master /app/

ENV RUST_LOG=info

EXPOSE 8080

CMD ["/app/skill-master", "--config", "/config/config.toml"]
