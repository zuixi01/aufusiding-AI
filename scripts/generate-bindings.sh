#!/bin/bash
# 生成 UniFFI Swift/Kotlin 绑定

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UDL_PATH="$PROJECT_ROOT/ffi/src/prometheus.udl"
OUT_DIR="${1:-$PROJECT_ROOT/ffi/generated}"

mkdir -p -- "$OUT_DIR/swift" "$OUT_DIR/kotlin"
# Resolve a caller-supplied relative output path before changing to the workspace.
OUT_DIR="$(cd -- "$OUT_DIR" && pwd)"

echo "=== 生成 UniFFI 绑定 ==="
echo "UDL: $UDL_PATH"
echo "输出目录: $OUT_DIR"

# Always use the generator locked with the Rust runtime. A global bindgen may have
# a different UniFFI contract/checksum version and generate incompatible bindings.
echo "使用仓库锁定版本生成 Swift/Kotlin 绑定..."
cd "$PROJECT_ROOT"
cargo run --quiet --locked --manifest-path "$PROJECT_ROOT/Cargo.toml" \
  -p prometheus-ffi --bin uniffi-bindgen-local -- "$UDL_PATH" "$OUT_DIR"

echo "完成。"
