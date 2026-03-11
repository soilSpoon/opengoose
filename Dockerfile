# syntax=docker/dockerfile:1.7

FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS deps
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json

FROM chef AS builder
COPY . .
COPY --from=deps /app/target /app/target
RUN cargo build --release --locked --package opengoose-cli

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        libgcc-s1 \
        libgomp1 \
        libssl3 \
        libstdc++6 \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --create-home --home-dir /home/opengoose --uid 10001 opengoose
WORKDIR /home/opengoose
COPY --from=builder /app/target/release/opengoose /usr/local/bin/opengoose
ENV HOME=/var/lib/opengoose \
    OPENGOOSE_HOST=0.0.0.0 \
    OPENGOOSE_PORT=8080 \
    OPENGOOSE_DB_PATH=/var/lib/opengoose/sessions.db \
    GOOSE_DISABLE_KEYRING=1 \
    RUST_LOG=info
RUN mkdir -p /var/lib/opengoose \
    && chown -R opengoose:opengoose /var/lib/opengoose /home/opengoose
VOLUME ["/var/lib/opengoose"]
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${OPENGOOSE_PORT}/api/health/live" >/dev/null || exit 1
USER opengoose
ENTRYPOINT ["opengoose"]
CMD ["web"]
