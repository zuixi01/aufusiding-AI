#!/usr/bin/env bash
# 开发环境初始化脚本

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

echo "=== Prometheus 开发环境初始化 ==="
echo ""

# 不隐式安装系统工具；请先完成 README 和 Tauri 官方前置依赖。
for tool in rustup cargo node npm; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "错误: 未找到 $tool。请先按 README 安装 Rust 和 Node.js 22（>=22.12）。"
        exit 1
    fi
done

# 安装 Rust 组件
echo "安装 Rust 组件..."
rustup component add rustfmt clippy

# 检查 cargo 工具
echo "检查 cargo 工具..."
cargo --version

echo "安装并构建桌面前端..."
npm --prefix apps/desktop ci
npm --prefix apps/desktop run build

# 编译检查
echo ""
echo "执行 cargo check..."
cargo check --workspace --locked

echo ""
echo "=== 开发环境初始化完成 ==="
echo ""
echo "常用命令："
echo "  cargo check --workspace --locked  - 编译检查"
echo "  cargo test --workspace --locked   - 运行测试"
echo "  cargo clippy --workspace --all-targets --locked -- -D warnings  - 代码检查"
echo "  cargo fmt            - 格式化代码"
echo "  cargo run -p prometheus-cli -- chat  - 启动 CLI"
