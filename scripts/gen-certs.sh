#!/usr/bin/env bash
# 生成开发环境自签名 TLS 证书
# 用法: ./scripts/gen-certs.sh [输出目录]

set -euo pipefail

CERT_DIR="${1:-./certs}"
DAYS=365
SUBJ="/C=CN/ST=Dev/L=Dev/O=Prometheus/CN=localhost"

mkdir -p "$CERT_DIR"

openssl req -x509 -nodes \
    -days "$DAYS" \
    -newkey rsa:2048 \
    -keyout "$CERT_DIR/server.key" \
    -out "$CERT_DIR/server.crt" \
    -subj "$SUBJ" \
    -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

chmod 600 "$CERT_DIR/server.key"
chmod 644 "$CERT_DIR/server.crt"

echo "证书已生成到 $CERT_DIR/"
echo "  - $CERT_DIR/server.crt (公钥)"
echo "  - $CERT_DIR/server.key (私钥)"
