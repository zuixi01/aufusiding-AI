#!/usr/bin/env bash
# Android NDK 交叉编译脚本
# 用法: ./scripts/build-android.sh [debug|release]
set -euo pipefail

PROFILE="${1:-debug}"
TARGETS=("aarch64-linux-android" "armv7-linux-androideabi" "x86_64-linux-android")

# 检查 NDK
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    echo "错误: 请设置 ANDROID_NDK_HOME 环境变量"
    exit 1
fi

# 安装 Rust 目标
for target in "${TARGETS[@]}"; do
    rustup target add "$target" 2>/dev/null || true
done

# 编译
BUILD_FLAG=""
if [ "$PROFILE" = "release" ]; then
    BUILD_FLAG="--release"
fi

for target in "${TARGETS[@]}"; do
    echo "==> 编译 $target ($PROFILE)"
    cargo build -p prometheus-ffi --target "$target" $BUILD_FLAG
done

echo "==> Android 编译完成"
echo "输出目录: target/<target>/$PROFILE/libprometheus_ffi.*"
