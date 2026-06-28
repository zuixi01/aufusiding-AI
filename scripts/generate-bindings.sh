#!/bin/bash
# 生成 UniFFI Swift/Kotlin 绑定

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UDL_PATH="$PROJECT_ROOT/ffi/src/prometheus.udl"
OUT_DIR="${1:-$PROJECT_ROOT/ffi/generated}"

mkdir -p "$OUT_DIR/swift" "$OUT_DIR/kotlin"

echo "=== 生成 UniFFI 绑定 ==="
echo "UDL: $UDL_PATH"
echo "输出目录: $OUT_DIR"

if command -v uniffi-bindgen >/dev/null 2>&1; then
  UNIFFI_CMD=(uniffi-bindgen)
elif [ -x "$HOME/.cargo/bin/uniffi-bindgen" ]; then
  UNIFFI_CMD=("$HOME/.cargo/bin/uniffi-bindgen")
else
  UNIFFI_CMD=()
fi

echo "生成 Swift 绑定..."
if [ ${#UNIFFI_CMD[@]} -gt 0 ]; then
  "${UNIFFI_CMD[@]}" generate "$UDL_PATH" --language swift --out-dir "$OUT_DIR/swift"
else
  "$HOME/.cargo/bin/cargo" run --quiet -p prometheus-ffi --bin uniffi-bindgen-local -- "$UDL_PATH" "$OUT_DIR"
fi

echo "生成 Kotlin 绑定..."
if [ ${#UNIFFI_CMD[@]} -gt 0 ]; then
  "${UNIFFI_CMD[@]}" generate "$UDL_PATH" --language kotlin --out-dir "$OUT_DIR/kotlin"
fi

echo "完成。"
