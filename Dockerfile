# --- Chef: cache dependency builds ---
FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# --- Plan: compute dependency fingerprint ---
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- Build: compile with cached deps ---
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release -p lineup_server

# --- Runtime: minimal image ---
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create data directory for SQLite files.
RUN mkdir -p /data /data/demos

COPY --from=builder /app/target/release/lineup_server /usr/local/bin/
COPY --from=builder /app/crates/server/public /app/public

ENV HOST=0.0.0.0
ENV PORT=8080
ENV MASTER_DB=/data/master.db
ENV DATA_DIR=/data
ENV PUBLIC_DIR=/app/public

EXPOSE 8080

CMD ["lineup_server"]
