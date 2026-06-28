#!/bin/bash
# iOS 构建脚本
# 编译 Rust 核心库为 iOS 静态库

set -e

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== 构建 Prometheus iOS 库 ==="

# 添加 iOS 目标
rustup target add aarch64-apple-ios
rustup target add aarch64-apple-ios-sim

# 编译 Release
echo "编译 aarch64-apple-ios..."
cargo build --release --target aarch64-apple-ios -p prometheus-ffi

echo "编译 aarch64-apple-ios-sim..."
cargo build --release --target aarch64-apple-ios-sim -p prometheus-ffi

# 生成 UniFFI 绑定
echo "生成 Swift 绑定..."
"$PROJECT_ROOT/scripts/generate-bindings.sh" "$PROJECT_ROOT/ffi/generated"

echo ""
echo "构建完成！"
echo "静态库: target/aarch64-apple-ios/release/libprometheus_ffi.a"
echo "Swift 绑定: ffi/generated/"
