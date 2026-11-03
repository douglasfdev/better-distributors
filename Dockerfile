FROM rust:stable-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown && cargo install trunk --version 0.21.14 --locked
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY backend ./backend
COPY shared ./shared
COPY frontend ./frontend
RUN cargo build --locked --release -p distributor-server
WORKDIR /app/frontend
RUN trunk build --release --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* && groupadd --gid 10001 app && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin app && install -d -o 10001 -g 10001 /data
WORKDIR /app
COPY --from=builder /app/target/release/distributor-server /usr/local/bin/distributor-server
COPY --from=builder /app/frontend/dist ./frontend/dist
COPY --from=builder /app/frontend/capture.html /app/frontend/bridge.js ./frontend/
ENV DATABASE_URL=sqlite:///data/distributors.db?mode=rwc \
    BIND_ADDR=0.0.0.0:3000 \
    CAPTURE_BIND_ADDR=0.0.0.0:3001 \
    FRONTEND_DIST=/app/frontend/dist \
    CAPTURE_FRONTEND_DIR=/app/frontend
VOLUME ["/data"]
USER 10001:10001
EXPOSE 3000 3001
CMD ["distributor-server"]
