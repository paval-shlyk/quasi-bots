FROM rust:1.87-bookworm AS chef

WORKDIR /build

RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef

FROM chef AS planner

COPY . .

RUN cargo chef prepare --bin finance-mcp --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /build/recipe.json recipe.json

# Build dependencies — cached until Cargo.toml changes
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

ARG GIT_COMMIT
RUN GIT_COMMIT=$GIT_COMMIT cargo build --release --bin finance-mcp && \
    strip target/release/finance-mcp

FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/target/release/finance-mcp /app/

ENV RUST_LOG=info

EXPOSE 9191

CMD ["/app/finance-mcp", "--config", "/config/config.toml"]
