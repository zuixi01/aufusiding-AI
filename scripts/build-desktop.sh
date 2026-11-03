#!/usr/bin/env bash
# 桌面平台编译脚本
# 用法: ./scripts/build-desktop.sh [debug|release]
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

PROFILE="${1:-debug}"
BUILD_FLAG=""
if [ "$PROFILE" = "release" ]; then
    BUILD_FLAG="--release"
elif [ "$PROFILE" != "debug" ]; then
    echo "用法: $0 [debug|release]" >&2
    exit 1
fi

OS="$(uname -s)"
echo "==> 编译桌面版 ($OS, $PROFILE)"

# 编译原生库和命令行程序；Tauri 安装包在 apps/desktop 使用 npm run tauri -- build -- --locked。
cargo build --locked -p prometheus-ffi -p prometheus-cli -p prometheus-server $BUILD_FLAG

echo "==> 桌面编译完成"
echo "输出目录: target/$PROFILE/"
