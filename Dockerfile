# ── Stage 1: Build ──
FROM rust:1.99.1-bookworm AS builder

WORKDIR /app
COPY . .

# 构建 release 二进制
RUN cargo build --locked --release -p prometheus-server \
    && strip target/release/prometheus-server

# ── Stage 2: Runtime ──
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# 非 root 用户
RUN useradd -m -s /bin/bash prometheus
USER prometheus

WORKDIR /home/prometheus

COPY --from=builder /app/target/release/prometheus-server /usr/local/bin/prometheus-server

# 数据目录
RUN mkdir -p /home/prometheus/data
ENV PROMETHEUS_DATA_DIR=/home/prometheus/data
ENV PROMETHEUS_HOST=0.0.0.0
ENV PROMETHEUS_PORT=3000
ENV PROMETHEUS_AUTH_ENABLED=true
ENV RUST_LOG=info

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/api/v1/health || exit 1

ENTRYPOINT ["prometheus-server"]
