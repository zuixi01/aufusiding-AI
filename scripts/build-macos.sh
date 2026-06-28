#!/bin/bash
# macOS 构建脚本
# 编译 Rust 核心库为 macOS 动态库/静态库

set -e

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== 构建 Prometheus macOS 库 ==="

# 添加 macOS 目标（Universal Binary）
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin

# 编译 Release
echo "编译 aarch64-apple-darwin..."
cargo build --release --target aarch64-apple-darwin -p prometheus-ffi

echo "编译 x86_64-apple-darwin..."
cargo build --release --target x86_64-apple-darwin -p prometheus-ffi

# 创建 Universal Binary
echo "创建 Universal Binary..."
mkdir -p "$PROJECT_ROOT/target/universal-apple-darwin/release"
lipo -create \
    "$PROJECT_ROOT/target/aarch64-apple-darwin/release/libprometheus_ffi.a" \
    "$PROJECT_ROOT/target/x86_64-apple-darwin/release/libprometheus_ffi.a" \
    -output "$PROJECT_ROOT/target/universal-apple-darwin/release/libprometheus_ffi.a"

# 生成 UniFFI 绑定
echo "生成 Swift 绑定..."
"$PROJECT_ROOT/scripts/generate-bindings.sh" "$PROJECT_ROOT/ffi/generated"

echo ""
echo "构建完成！"
echo "Universal 静态库: target/universal-apple-darwin/release/libprometheus_ffi.a"
