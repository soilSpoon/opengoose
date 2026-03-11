FROM rust:1.85-bookworm AS chef
WORKDIR /app
RUN cargo install cargo-chef --locked

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS dependencies
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json

FROM chef AS builder
COPY . .
COPY --from=dependencies /app/target /app/target
RUN cargo build --release --locked --package opengoose-cli

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libgcc-s1 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --uid 10001 --home-dir /var/lib/opengoose opengoose

WORKDIR /app

COPY --from=builder /app/target/release/opengoose /usr/local/bin/opengoose

ENV HOME=/var/lib/opengoose
ENV OPENGOOSE_HOST=0.0.0.0
ENV OPENGOOSE_PORT=8080
ENV OPENGOOSE_DB_PATH=/var/lib/opengoose/.opengoose/sessions.db
ENV RUST_LOG=info

RUN mkdir -p /var/lib/opengoose \
    && chown -R opengoose:opengoose /var/lib/opengoose /app

VOLUME ["/var/lib/opengoose"]
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -fsS "http://127.0.0.1:${OPENGOOSE_PORT}/api/health/live" >/dev/null || exit 1

USER opengoose

ENTRYPOINT ["opengoose"]
CMD ["web"]
