FROM rust:1.93-trixie AS builder

WORKDIR /build

RUN apt-get update && \
    apt-get install -y \
    protobuf-compiler libsqlite3-dev && \
    rm -rf /var/lib/apt/lists/*

COPY . .

# Native build on the current platform
RUN cargo build --release -p skill-master

FROM debian:stable-slim AS runtime

WORKDIR /app

# The binary will be in the standard target/release directory
COPY --from=builder /build/target/release/skill-master /app/

ENV RUST_LOG=info

EXPOSE 8080

CMD ["/app/skill-master", "--config", "/config/config.toml"]
