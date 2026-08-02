# syntax=docker/dockerfile:1
# -------------------------------------------------------------------
# Stage 1: build all workspace binaries
# -------------------------------------------------------------------
FROM rust:1.85-bookworm AS builder

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY communication/ communication/
COPY crypto/ crypto/
COPY crypto-master/ crypto-master/

# Stub out workspace members we do not need so cargo resolves the workspace
# but does not try to compile unrelated crates.
RUN for dir in finance knowledge monitor news skill-master telemetry; do \
      mkdir -p "$dir/src" && \
      echo '[package]\nname = "'"$dir"'"\nversion = "0.1.0"\nedition = "2024"' > "$dir/Cargo.toml" && \
      echo "" > "$dir/src/lib.rs"; \
    done

RUN cargo build --release --package crypto --package crypto-master

# -------------------------------------------------------------------
# Stage 2: minimal runtime image
# -------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/crypto        /usr/local/bin/crypto
COPY --from=builder /src/target/release/crypto-master  /usr/local/bin/crypto-master
COPY --from=builder /src/target/release/crypto-cli     /usr/local/bin/crypto-cli

# Default: run the crypto worker
CMD ["crypto"]
