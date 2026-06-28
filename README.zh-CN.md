<div align="center">
  <img src="assets/logo.png" width="140" height="140" alt="Prometheus AI"/>

  # Prometheus AI

  **具备智能路由能力的本地优先 AI 助手**

  [![Rust](https://img.shields.io/badge/Rust-1.93+-orange?logo=rust)](https://www.rust-lang.org)
  [![License](https://img.shields.io/badge/License-MIT-blue)](LICENSE)
  [![CI](https://img.shields.io/github/actions/workflow/status/5201314wzq1023-alt/prometheus-ai/ci.yml?branch=main&logo=github-actions&label=CI)](https://github.com/5201314wzq1023-alt/prometheus-ai/actions/workflows/ci.yml)
  [![Tests](https://img.shields.io/badge/Tests-2731%20passing-success)](core/tests)
  [![Platform](https://img.shields.io/badge/Platform-iOS%20%7C%20macOS%20%7C%20Android%20%7C%20Windows%20%7C%20Linux-green)]()

</div>

<p align="center">
  <a href="./README.md">
    <img src="https://img.shields.io/badge/Language-English-111111?style=for-the-badge" alt="English"/>
  </a>
  <a href="./README.zh-CN.md">
    <img src="https://img.shields.io/badge/%E8%AF%AD%E8%A8%80-%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-111111?style=for-the-badge" alt="简体中文"/>
  </a>
</p>

---

## 项目概览

Prometheus AI 是一个以本地推理为优先、并可在必要时无缝接入云端模型的智能助手。项目使用 Rust 构建，兼顾性能与安全，包含自适应路由、多层记忆系统和经济性优化能力。

**核心理念**：让 80% 以上的对话在本地运行（零成本），仅在必要时将复杂任务路由到云端 API，并让用户完全掌控自己的数据。

---

## 当前状态（2026 年 3 月）

### P0-P6 + P2 + P3 已完成 ✓

**核心引擎**（P0-P6，229 个测试）：
- Ollama 本地推理集成（`qwen2.5:1.5b` + `all-minilm`）
- 智能路由系统（意图分类 + 模型选择）
- 记忆系统（工作记忆 + 情景记忆 + 知识库）
- Daemon Mind（预测、信念、反思模块）
- 经济大脑（成本跟踪 + ROI 优化）
- 端到端集成测试

**生产层**（P2，新增 77 个测试，总计 306 个）：
- CLI 工具：交互式 REPL、会话管理、记忆命令
- HTTP API：RESTful 接口（聊天、记忆、统计），Swagger UI 位于 `/api/docs`
- 认证：API Key + JWT 双模式，支持三种权限级别
- 性能优化：向量缓存、WAL 模式、并发与请求体限制
- 监控：Prometheus 指标（`/metrics`）、结构化 JSON 日志、深度健康检查
- 插件：计算器（表达式解析）与天气（模拟）插件，含 `PluginManager`
- FFI：UniFFI 绑定，为 Swift/Kotlin 提供记忆系统 API
- 部署：Docker 多阶段构建、`docker-compose`、CI/CD 流水线

**高级能力**（P3，新增 83 个测试，总计 389 个）：
- WASM 插件运行时：基于 `wasmi` 的沙箱执行，`WasmPluginExecutor`，带权限门控
- 语音 I/O 抽象：STT/TTS/唤醒词 traits、mock 引擎、`VoicePipeline`
- GDPR 合规：同意管理、数据导出（可携带权）、数据删除（被遗忘权）、PII 匿名化
- FFI 生命周期 API：暂停、恢复、关闭，设置读取/写入，移动端隐私操作
- 构建脚本：Android NDK 交叉编译（aarch64/armv7/x86_64），桌面端（Linux/macOS/Windows）

---

## 核心特性

### 智能路由

基于任务复杂度的自适应模型选择：

```
用户输入 → 意图分类（本地，<1ms）
  ├─ 闲聊 / 简单任务 → 本地 3B/7B 模型（免费）
  ├─ 中等任务       → 本地 14B 或云端预算模型
  └─ 复杂任务       → 按需调用高质量云端模型
```

**实现方式**：`IntentClassifier` 使用 LLM 驱动分类（并带规则回退），`Router` 提供 Balanced / CostOptimized / QualityFirst 等多种策略，`CapabilityMatrix` 负责动态模型能力匹配。

### 记忆系统

用于上下文管理的多层记忆架构：

- **工作记忆**：基于艾宾浩斯遗忘曲线的短期上下文
- **情景记忆**：支持向量检索的对话历史（`sqlite-vec`）
- **知识库**：文档存储与混合检索（向量 + BM25 + RRF）
- **Embedding 服务**：可插拔抽象（Ollama / Mock），384 维向量

**实现方式**：`WorkingMemory`、`EpisodicMemory`、`KnowledgeBase`，并通过 `EmbeddingService` 进行依赖注入。

### Daemon Mind

提供主动智能能力的认知模块：

- **预测模块**：PBRIC 循环（Predict → Belief → Reflect → Intent → Curiosity）
- **信念模块**：使用贝叶斯推断建模用户偏好
- **反思模块**：由 LLM 驱动的对话质量评估（带规则回退）

**实现方式**：`PredictModule`、`BeliefModule`、`ReflectionModule`，并可选接入 LLM provider。

### 经济大脑

负责成本跟踪与 ROI 优化：

- 实时成本跟踪，带 EMA 平滑
- 按模型统计（调用次数、平均成本、平均时延、满意度）
- ROI 计算：`satisfaction / (cost + latency_penalty)`
- 可配置的日 / 月预算管理

**实现方式**：`EconomicCortex` 通过 `CostRecord` 跟踪成本，并使用 `ModelStats` 聚合统计。

---

## 架构

```
┌─────────────────────────────────────────────────────────┐
│                      应用层                              │
│   CLI · HTTP API · iOS/macOS · Android · Windows/Linux  │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                     Rust 核心引擎                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   Routing    │  │    Memory    │  │  Daemon Mind │  │
│  │  Classifier  │  │   Working    │  │   Predict    │  │
│  │   Router     │  │  Episodic    │  │   Belief     │  │
│  │ Capability   │  │  Knowledge   │  │   Reflect    │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │  Providers   │  │  Economics   │  │   Plugins    │  │
│  │   Ollama     │  │ Cost Track   │  │    WASM      │  │
│  │   OpenAI     │  │ ROI Optimize │  │   Sandbox    │  │
│  │  Anthropic   │  │   Budget     │  │              │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│  ┌──────────────┐  ┌──────────────┐                     │
│  │    Voice     │  │   Privacy    │                     │
│  │  STT/TTS     │  │   Consent    │                     │
│  │  Wakeword    │  │  Export/Del  │                     │
│  │  Pipeline    │  │  Anonymize   │                     │
│  └──────────────┘  └──────────────┘                     │
└─────────────────────────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                    可选云端服务                          │
│        Anthropic · OpenAI · Google · Qwen · Doubao      │
└─────────────────────────────────────────────────────────┘
```

---

## 技术栈

| 组件 | 技术 | 选型原因 |
|------|------|----------|
| Core Engine | Rust, Tokio | 高性能、内存安全、异步运行时 |
| Local Inference | Ollama (llama.cpp) | 支持 GGUF 量化，适配 Metal/CUDA 加速 |
| Vector Search | sqlite-vec | 嵌入式向量数据库 |
| Embedding | all-minilm (384-dim) | 轻量级、多语言 |
| Storage | SQLite | 嵌入式、ACID、跨平台 |
| Testing | cargo test, httpmock | 389 个测试，0 失败 |
| CI/CD | GitHub Actions | 测试、clippy、fmt、Docker 构建、安全审计 |

---

## 项目结构

```
prometheus-ai/
├── core/                 # Rust 核心引擎
│   ├── src/
│   │   ├── router/       # 意图分类、路由、能力矩阵
│   │   ├── memory/       # 工作记忆、情景记忆、知识库
│   │   ├── mind/         # Daemon Mind（预测、信念、反思、经济）
│   │   ├── providers/    # LLM providers（Ollama、OpenAI、Anthropic）
│   │   ├── plugins/      # 插件系统（管理、注册、沙箱、WASM 运行时）
│   │   ├── voice/        # 语音 I/O（STT/TTS/唤醒词、mock、pipeline）
│   │   └── privacy/      # GDPR（同意、导出、删除、匿名化）
│   └── tests/            # 单元测试、集成测试、端到端测试
├── ffi/                  # UniFFI 绑定（Rust → Swift/Kotlin）
├── cli/                  # CLI 工具（交互式 REPL、会话管理）
├── server/               # HTTP API 服务（axum、OpenAPI、auth、metrics）
├── apps/                 # 原生应用（规划中）
│   ├── macos/            # macOS（SwiftUI）
│   ├── ios/              # iOS（SwiftUI）
│   ├── android/          # Android（Jetpack Compose）
│   └── desktop/          # Windows/Linux（Tauri）
├── plugins/              # 插件生态
│   ├── sdk/              # 插件开发 SDK
│   ├── calculator/       # 内置计算器插件
│   └── weather/          # 内置天气插件
├── docs/                 # 文档
│   ├── plans/            # 开发计划（P1、P2、...）
│   ├── reports/          # 完成报告
│   └── engineering/      # 工程文档
└── assets/               # Logo、图标、资源文件
```

---

## 快速开始

### 前置要求

- Rust 1.93+（[安装](https://rustup.rs)）
- Ollama（[安装](https://ollama.com)）

### 安装与启动

```bash
# 克隆仓库
git clone https://github.com/5201314wzq1023-alt/prometheus-ai.git
cd prometheus-ai

# 安装 Ollama 模型
ollama pull qwen2.5:1.5b
ollama pull all-minilm

# 运行测试
cargo test --workspace

# 启动 CLI（交互模式）
cargo run --bin prometheus-cli

# 启动 HTTP API 服务
cargo run --bin prometheus-server
# API 文档: http://localhost:3000/api/docs
# 健康检查: http://localhost:3000/api/v1/health
# 指标:     http://localhost:3000/metrics
```

### Docker 部署

```bash
# 使用 Docker Compose 构建并启动
docker compose up -d

# 或手动构建
docker build -t prometheus-ai .
docker run -p 3000:3000 -v prometheus-data:/home/prometheus/data prometheus-ai
```

### 运行测试

```bash
# 所有测试（不含 Ollama 集成测试）
cargo test --workspace

# Ollama 集成测试（需要本地已运行 Ollama）
cargo test --workspace -- --ignored

# 指定测试套件
cargo test --test e2e_integration_test

# 生成覆盖率
cargo tarpaulin --workspace --out Html
```

### 代码质量

```bash
# 静态检查
cargo clippy --workspace --all-targets -- -D warnings

# 格式检查
cargo fmt --all -- --check

# 安全审计
cargo audit
```

---

## 开发路线图

| 阶段 | 周期 | 状态 | 关键交付 |
|------|------|------|----------|
| **P0-P6** | 3 周 | ✅ 完成 | 核心引擎、Ollama 集成、路由、记忆、Daemon Mind、经济系统、端到端测试 |
| **P2** | 2 周 | ✅ 完成 | CLI、HTTP API、认证、性能、监控、插件、FFI、Docker、CI/CD |
| **P3** | 2 周 | ✅ 完成 | WASM 插件、语音 I/O 抽象、GDPR 合规、Android/桌面端 FFI |
| **P4** | TBD | ⏳ 未来 | IoT 集成、屏幕代理、多设备同步、高级个性化 |

**当前重点**：P4，IoT 集成与高级个性化。

---

## 性能指标

当前基准（P0-P6 基线）：

| 指标 | 目标 | 当前 |
|------|------|------|
| 意图分类 | < 1ms | ✓ 规则引擎 + LLM 回退 |
| 向量搜索（1K 文档） | < 50ms | ✓ 8.8ms（缓存命中） |
| 知识检索 | < 50ms | ✓ 2.4ms |
| 工作记忆 | < 1ms | ✓ 67µs |
| 并发吞吐 | > 15 QPS | ✓ 64 并发上限 |
| 空闲内存占用 | < 200MB | ✓ 已验证 |
| 测试覆盖 | > 80% | ✓ 389 个测试通过 |

---

## HTTP API 接口

基础地址：`http://localhost:3000`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/health` | 基础健康检查 |
| GET | `/api/v1/health/deep` | 深度健康检查（各子系统状态） |
| GET | `/api/v1/info` | 服务信息（名称、版本） |
| POST | `/api/v1/sessions` | 创建会话 |
| GET | `/api/v1/sessions` | 列出所有会话 |
| POST | `/api/v1/sessions/{id}/chat` | 发送消息 |
| GET | `/api/v1/sessions/{id}/messages` | 获取消息历史 |
| POST | `/api/v1/memory/search` | 搜索情景记忆 |
| POST | `/api/v1/memory/knowledge` | 添加知识 |
| GET | `/api/v1/memory/knowledge` | 搜索知识库 |
| POST | `/api/v1/memory/working` | 添加工作记忆 |
| GET | `/api/v1/memory/working` | 获取工作记忆 |
| GET | `/api/v1/stats/sessions` | 会话统计 |
| GET | `/api/v1/stats/overview` | 系统概览 |
| POST | `/api/v1/privacy/consent` | 记录用户同意 |
| GET | `/api/v1/privacy/export` | 导出用户数据（GDPR 数据可携带权） |
| DELETE | `/api/v1/privacy/data` | 删除用户数据（GDPR 被遗忘权） |
| POST | `/api/v1/privacy/anonymize` | 文本 PII 匿名化 |
| GET | `/metrics` | Prometheus 指标 |
| GET | `/api/docs` | Swagger UI |

## 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `PROMETHEUS_HOST` | `0.0.0.0` | 监听地址 |
| `PROMETHEUS_PORT` | `3000` | 监听端口 |
| `PROMETHEUS_DATA_DIR` | `/tmp/prometheus-server` | 数据存储目录 |
| `PROMETHEUS_LOG_FORMAT` | `text` | 日志格式（`text` / `json`） |
| `PROMETHEUS_AUTH_ENABLED` | `false` | 是否启用认证 |
| `PROMETHEUS_API_KEYS` | - | API Key 列表（逗号分隔） |
| `PROMETHEUS_JWT_SECRET` | - | JWT 签名密钥 |
| `PROMETHEUS_PLUGIN_DIR` | `./plugins` | WASM 插件目录 |
| `PROMETHEUS_CONSENT_DB` | `./consent.db` | GDPR 同意记录数据库路径 |
| `RUST_LOG` | `info` | 日志级别 |

---

## 文档

### 开发计划
- [P1 Execution Plan](docs/P1_EXECUTION_PLAN.md) - Ollama 集成路线图
- [P2 Development Plan](docs/plans/20260307-P2-PLAN.md) - 生产层与应用层
- [P3 Development Plan](docs/plans/20260309-P3-PLAN.md) - WASM 插件、语音 I/O、GDPR、FFI

### 完成报告
- [P0-P5 Completion Report](docs/reports/2026-03-06-P1-P5-completion.md)
- [P6 E2E Testing Report](docs/reports/2026-03-06-P6-completion.md)

### 设计文档
- [Product Design](docs/product-design.md) - 完整产品愿景（20 章）
- [Algorithm Design](docs/ALGORITHM_DESIGN.md) - 核心算法与数据结构
- [Architecture Overview](docs/architecture.md) - 系统架构
- [Local Model Deployment](docs/LOCAL_MODEL_DEPLOYMENT.md) - Ollama 安装与部署指南

### API 参考
- [API Documentation](docs/api-reference.md) - 核心 API 参考
- [Contributing Guide](docs/contributing.md) - 贡献说明

---

## 参与贡献

欢迎贡献。具体规则见 [CONTRIBUTING.md](docs/contributing.md)。

### 开发流程

1. Fork 本仓库
2. 新建功能分支（`git checkout -b feature/amazing-feature`）
3. 修改代码并补齐测试
4. 运行质量检查（`cargo test && cargo clippy && cargo fmt`）
5. 使用规范化提交信息（例如 `feat: add amazing feature`）
6. 推送分支并创建 Pull Request

### 代码标准

- 所有代码必须通过 `cargo clippy`，且零 warning
- 所有代码必须通过 `cargo fmt` 格式化
- 所有新功能必须附带测试（目标：80%+ 覆盖率）
- 所有公开 API 必须提供 rustdoc 文档

---

## 许可证

本项目基于 MIT License 开源，详见 [LICENSE](LICENSE)。

---

## 致谢

- **Ollama** - 本地 LLM 推理引擎
- **llama.cpp** - 高性能 LLM 运行时
- **sqlite-vec** - 嵌入式向量检索
- **Rust Community** - 出色的生态与工具链

---

<div align="center">

**Prometheus AI** — 把 AI 的火种带给每一个人

[Documentation](docs/) · [Report Bug](https://github.com/5201314wzq1023-alt/prometheus-ai/issues) · [Request Feature](https://github.com/5201314wzq1023-alt/prometheus-ai/issues)

</div>
