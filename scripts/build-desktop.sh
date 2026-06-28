#!/usr/bin/env bash
# 桌面平台编译脚本
# 用法: ./scripts/build-desktop.sh [debug|release]
set -euo pipefail

PROFILE="${1:-debug}"
BUILD_FLAG=""
if [ "$PROFILE" = "release" ]; then
    BUILD_FLAG="--release"
fi

OS="$(uname -s)"
echo "==> 编译桌面版 ($OS, $PROFILE)"

# 编译 FFI 库
cargo build -p prometheus-ffi $BUILD_FLAG

# 编译 CLI
cargo build -p prometheus-cli $BUILD_FLAG

# 编译 Server
cargo build -p prometheus-server $BUILD_FLAG

echo "==> 桌面编译完成"
echo "输出目录: target/$PROFILE/"
