#!/bin/bash
# 开发环境初始化脚本

set -e

echo "=== Prometheus 开发环境初始化 ==="
echo ""

# 检查 Rust
if command -v rustc &> /dev/null; then
    echo "[OK] Rust $(rustc --version | awk '{print $2}')"
else
    echo "[!] 未检测到 Rust，正在安装..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# 安装 Rust 组件
echo "安装 Rust 组件..."
rustup component add rustfmt clippy

# 检查 cargo 工具
echo "检查 cargo 工具..."
cargo --version

# 编译检查
echo ""
echo "执行 cargo check..."
cargo check --workspace

echo ""
echo "=== 开发环境初始化完成 ==="
echo ""
echo "常用命令："
echo "  cargo check          - 编译检查"
echo "  cargo test           - 运行测试"
echo "  cargo clippy         - 代码检查"
echo "  cargo fmt            - 格式化代码"
echo "  cargo run -p prometheus-cli -- chat  - 启动 CLI"
