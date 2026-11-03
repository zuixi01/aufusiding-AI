#!/usr/bin/env bash
# Android NDK 交叉编译脚本
# 用法: ./scripts/build-android.sh [debug|release]
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

PROFILE="${1:-debug}"
TARGETS=("aarch64-linux-android" "armv7-linux-androideabi" "x86_64-linux-android")

# 检查 NDK
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    echo "错误: 请设置 ANDROID_NDK_HOME 环境变量"
    exit 1
fi

# 安装 Rust 目标
for target in "${TARGETS[@]}"; do
    rustup target add "$target"
done

# 编译
BUILD_FLAG=""
if [ "$PROFILE" = "release" ]; then
    BUILD_FLAG="--release"
elif [ "$PROFILE" != "debug" ]; then
    echo "用法: $0 [debug|release]" >&2
    exit 1
fi

for target in "${TARGETS[@]}"; do
    echo "==> 编译 $target ($PROFILE)"
    cargo build --locked -p prometheus-ffi --target "$target" $BUILD_FLAG
done

echo "==> Android 编译完成"
echo "输出目录: target/<target>/$PROFILE/libprometheus_ffi.*"
