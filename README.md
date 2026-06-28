# aufusiding-AI

<div align="center">
  <img src="assets/logo.png" width="128" height="128" alt="Prometheus AI logo">

  <h3>Local-first AI assistant and ambient intelligence platform</h3>

  <p>
    <a href="https://github.com/zuixi01/aufusiding-AI/actions"><img alt="GitHub Actions" src="https://img.shields.io/github/actions/workflow/status/zuixi01/aufusiding-AI/ci.yml?branch=main&label=CI&logo=github-actions"></a>
    <a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-MIT-blue"></a>
    <a href="https://www.rust-lang.org"><img alt="Rust" src="https://img.shields.io/badge/Rust-2021-orange?logo=rust"></a>
    <img alt="Platforms" src="https://img.shields.io/badge/platform-macOS%20%7C%20iOS%20%7C%20Android%20%7C%20Desktop%20%7C%20Server-green">
  </p>

  <p>
    <a href="README.en.md">English</a> ·
    <a href="README.zh-CN.md">简体中文</a> ·
    <a href="README.ja.md">日本語</a> ·
    <a href="README.fr.md">Français</a>
  </p>
</div>

## 项目简介

aufusiding-AI 是一个以 Rust 为核心的本地优先 AI 助手项目，代码中也使用 Prometheus AI 作为产品名。项目目标是在本地完成尽可能多的推理、记忆、语音、IoT 和工具调用能力，并在必要时路由到云端模型，从而兼顾隐私、成本和复杂任务质量。

仓库包含核心引擎、命令行工具、HTTP 服务、桌面端、移动端桥接、插件 SDK、内置插件、FFI 绑定、Docker 部署和工程文档。

## 核心能力

- 本地优先推理：优先接入 Ollama / llama.cpp 等本地模型，复杂任务再按策略转交云端 Provider。
- 智能路由：根据任务复杂度、模型能力、预算和质量偏好选择执行路径。
- 多层记忆：包含工作记忆、情景记忆、知识库、用户画像和向量检索能力。
- Agent 编排：提供计划、执行、质量门禁、角色身份、通信与共识相关模块。
- 语音与环境智能：包含 STT、TTS、唤醒词、VAD、常驻对话、主动建议和安全模式。
- IoT 控制：覆盖设备发现、自动化、场景执行、车辆集成、地理围栏和多协议适配。
- 隐私与安全：包含 PII 检测、匿名化、同意管理、数据导出、删除、审计和沙箱权限。
- 插件系统：提供 WASM 沙箱、插件运行时、插件 SDK，以及天气、计算器等内置插件示例。
- 跨平台入口：提供 CLI、HTTP API、Tauri 桌面端、SwiftUI iOS/macOS、Android Compose 和 UniFFI 绑定。

## 架构概览

```text
┌─────────────────────────────────────────────────────────────┐
│                         应用层                              │
│ CLI · HTTP API · Tauri Desktop · iOS/macOS · Android         │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│                       Rust 核心引擎                          │
│ Routing · Memory · Providers · Agent · Voice · IoT · Privacy │
│ Plugins · Safety · Session · Storage · Ambient Intelligence  │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│                     本地与云端执行层                         │
│ Ollama / llama.cpp · OpenAI · Anthropic · Qwen · HTTP backend │
└─────────────────────────────────────────────────────────────┘
```

更多架构细节见 [docs/architecture.md](docs/architecture.md)。

## 仓库结构

```text
.
├── core/                 # Rust 核心引擎：路由、记忆、Agent、语音、IoT、隐私、安全
├── cli/                  # 命令行入口，二进制名为 prometheus
├── server/               # Axum HTTP API 服务，二进制名为 prometheus-server
├── apps/
│   ├── desktop/          # Tauri 桌面端
│   ├── ios/              # iOS SwiftUI 应用与 Rust 桥接
│   ├── macos/            # macOS SwiftUI 应用与 Rust 桥接
│   └── android/          # Android Compose 应用与 Rust 桥接
├── ffi/                  # UniFFI 定义与生成绑定
├── plugins/              # 插件 SDK 和内置插件
├── docs/                 # 架构、API、设计、计划和研究文档
├── scripts/              # 构建、证书、绑定生成和平台脚本
├── tests/                # 集成测试与基准测试入口
├── Dockerfile
└── docker-compose.yml
```

## 快速开始

### 环境要求

- Rust stable toolchain
- Docker 与 Docker Compose，可选
- Bun，用于桌面端前端依赖和脚本
- Ollama，可选，用于本地模型推理

### 获取代码

```bash
git clone https://github.com/zuixi01/aufusiding-AI.git
cd aufusiding-AI
```

### 构建和测试 Rust workspace

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

### 运行 CLI

```bash
cargo run -p prometheus-cli -- --help
cargo run -p prometheus-cli -- chat
```

### 运行 HTTP 服务

```bash
cargo run -p prometheus-server
```

默认 Docker 配置使用 `3000` 端口。服务启动后可查看：

- 健康检查：`http://localhost:3000/api/v1/health`
- API 文档：`http://localhost:3000/api/docs`
- Prometheus 指标：`http://localhost:3000/metrics`

### 使用 Docker Compose

```bash
export PROMETHEUS_JWT_SECRET="change-me"
docker compose up --build
```

### 运行桌面端

```bash
cd apps/desktop
bun install
bun run tauri dev
```

如果本机 Bun 与某些原生依赖不兼容，可回退到项目现有 Node/npm 工具链，同时保留已有 lock 文件。

## 配置说明

常用环境变量：

| 变量 | 说明 | 示例 |
| --- | --- | --- |
| `PROMETHEUS_HOST` | HTTP 服务监听地址 | `0.0.0.0` |
| `PROMETHEUS_PORT` | HTTP 服务端口 | `3000` |
| `PROMETHEUS_LOG_FORMAT` | 日志格式 | `json` |
| `PROMETHEUS_AUTH_ENABLED` | 是否启用认证 | `true` |
| `PROMETHEUS_JWT_SECRET` | JWT 签名密钥 | `change-me` |
| `PROMETHEUS_API_KEYS` | API Key 列表 | `key1,key2` |
| `PROMETHEUS_CORS_ORIGINS` | CORS 来源白名单 | `http://localhost:5173` |
| `RUST_LOG` | Rust 日志级别 | `info` |

## 文档入口

- [架构说明](docs/architecture.md)
- [API 参考](docs/api-reference.md)
- [本地模型部署](docs/LOCAL_MODEL_DEPLOYMENT.md)
- [产品设计](docs/product-design.md)
- [开发计划](docs/DEVELOPMENT_PLAN_2026Q1.md)
- [UI/UX 规范](docs/design/UI_UX_SPECIFICATION.md)
- [贡献指南](CONTRIBUTING.md)
- [安全策略](SECURITY.md)

## 开发约定

- Rust 代码使用 `cargo fmt`、`cargo clippy` 和 `cargo test` 做基础质量门禁。
- JS/TS 子项目优先使用 Bun；若项目已有明确包管理器约定或 Bun 不兼容，再回退到对应工具。
- 不提交密钥、模型权重、数据库文件或本地生成的证书。
- 修改跨平台 API 时，同步检查 `ffi/`、移动端桥接代码和相关文档。

## 许可证

本项目使用 MIT License，详见 [LICENSE](LICENSE)。
