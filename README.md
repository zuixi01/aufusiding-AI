# aufusiding-AI

<p align="center">
  <img src="assets/logo.png" width="128" height="128" alt="Prometheus AI logo">
</p>

<p align="center">基于 Rust 的本地优先 AI 助手 · Early Alpha</p>

<p align="center">
  <a href="https://github.com/zuixi01/aufusiding-AI/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/zuixi01/aufusiding-AI/ci.yml?branch=main&label=CI"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-blue"></a>
  <img alt="Development status: alpha" src="https://img.shields.io/badge/status-alpha-orange">
</p>

<p align="center">
  简体中文 · <a href="README.en.md">English</a> · <a href="README.ja.md">日本語</a> · <a href="README.fr.md">Français</a>
</p>

aufusiding-AI 探索在本机完成 AI 对话、会话存储和任务路由，并为需要时接入云端模型提供模块。仓库内部的 crate、二进制、配置目录和应用仍使用 **Prometheus AI** 名称；例如 CLI 二进制名是 `prometheus`。

**当前是面向开发者的早期 Alpha。** 建议从 CLI + Ollama 开始体验。HTTP 服务、桌面端、移动端、语音和 IoT 的接入程度不同；有些入口使用模拟数据或仍需手动集成。接口和数据格式可能变化，暂不承诺生产可用、法规合规、性能指标或所有平台均可运行。请先看 [功能状态与已知限制](docs/project-status.md)。

## 可以从哪里开始

| 入口 | 当前用途 |
| --- | --- |
| CLI + Ollama | 本地对话、交互式 REPL、会话历史与导出；推荐体验入口 |
| Rust core | 路由、记忆、Provider、Agent、隐私、插件等模块的开发与实验 |
| HTTP API | 会话、对话、记忆等接口；需配置 Provider、数据目录和访问控制 |
| 桌面端 / 移动端 / FFI | 开发中的客户端与桥接代码，需要各平台工具链和进一步验证 |
| 语音 / IoT / 环境智能 | 实验性模块；源码存在不代表已接入所有客户端 |

## 快速开始：本地 CLI

需要 Git、rustup（使用仓库固定的 Rust **1.99.1** 和 Cargo）、本机 C/C++ 链接工具以及 [Ollama](https://ollama.com/)。CLI 不依赖 Docker、Node.js 或 Bun。固定工具链是工程基线，不是已验证的最低 Rust 版本。

```bash
git clone https://github.com/zuixi01/aufusiding-AI.git
cd aufusiding-AI
```

启动 Ollama；如果桌面应用已经提供本地服务，可跳过 `ollama serve`。它会持续占用当前终端，请在另一个终端继续：

```bash
ollama serve
```

下载模型并明确指定 Ollama 的模型标签：

```bash
ollama pull qwen2.5:1.5b
ollama list
cargo run --locked -p prometheus-cli -- --help
cargo run --locked -p prometheus-cli -- ask "你好，请用一句话介绍自己" --model qwen2.5:1.5b
```

交互式对话：

```bash
cargo run --locked -p prometheus-cli -- chat
```

在 REPL 输入 `/switch qwen2.5:1.5b` 选择已安装模型，输入 `/help` 查看命令，`/quit` 退出。模型响应速度、内存占用和下载大小取决于硬件与模型；本项目不附带模型权重。

没有 Ollama 时，可以明确选择离线模拟模式验证 CLI 流程：

```bash
cargo run --locked -p prometheus-cli -- --mock ask "你好"
```

模拟回复不经过真实模型。真实模式需要 Ollama 可达，默认地址为 `http://localhost:11434`；请用 `ollama list` 查看安装的模型，CLI 的 `model list` 和 `model test` 尚不能验证真实安装或连通性。

## 配置与其他入口

```bash
cargo run --locked -p prometheus-cli -- config list
cargo run --locked -p prometheus-cli -- config set default_model qwen2.5:1.5b
cargo run --locked -p prometheus-cli -- session list
```

`config list` 显示配置文件路径和数据目录。CLI 在本机保存会话数据库与 REPL 历史，分享日志或备份前请检查个人数据。配置字段的实际生效范围、Windows/macOS/Linux 路径、HTTP 服务环境变量及常见问题见 [入门指南](docs/getting-started.md)。

桌面端使用仓库已有的 npm 锁文件与脚本；还需要对应平台的 Tauri 系统依赖：

```bash
cd apps/desktop
npm ci
npm run build
npm run tauri dev
```

桌面端属于实验性入口。HTTP 服务的本机启动示例和认证说明见 [入门指南](docs/getting-started.md#http-服务本机开发)。不要把开发配置直接作为公网部署配置。

## 仓库结构

```text
core/                 Rust 核心模块
cli/                  命令行入口 prometheus
server/               Axum HTTP 服务 prometheus-server
apps/desktop/         Tauri 桌面端
apps/ios/             iOS SwiftUI 客户端
apps/macos/           macOS SwiftUI 客户端
apps/android/         Android Compose 客户端
ffi/                  UniFFI 桥接
plugins/              插件 SDK 与内置示例
docs/                 入门、架构、设计与研究资料
scripts/              构建与平台辅助脚本
```

## 参与贡献

欢迎提供可复现 Bug、修正文档、补充实际平台验证，以及用测试补齐现有功能。请从 [贡献指南](CONTRIBUTING.md) 开始；提交问题时使用 [Issue 模板](https://github.com/zuixi01/aufusiding-AI/issues/new/choose)，报告安全问题前阅读 [安全策略](SECURITY.md)。

常见的 CLI 开发检查如下；完整检查范围按改动和 CI 确定，详见贡献指南：

```bash
cargo fmt --all -- --check
cargo check --locked -p prometheus-cli
cargo test --locked -p prometheus-cli
```

## 文档

- [入门与配置](docs/getting-started.md)
- [功能状态与已知限制](docs/project-status.md)
- [架构说明](docs/architecture.md)
- [API 参考](docs/api-reference.md)
- [本地模型部署资料](docs/LOCAL_MODEL_DEPLOYMENT.md)
- [贡献指南](CONTRIBUTING.md) · [行为准则](CODE_OF_CONDUCT.md) · [安全策略](SECURITY.md)

架构、设计和历史计划描述了项目方向，可能包含尚未接入的功能。判断当前行为时，以源码、功能状态文档和对应提交的 CI 结果为准。

## 许可证

代码采用 [MIT License](LICENSE)。外部模型、数据、服务和第三方依赖分别受其许可证或服务条款约束；随仓库分发的字体许可见 [第三方声明](THIRD_PARTY_NOTICES.md)。
