# aufusiding-AI

基于 Rust 的本地优先 AI 助手。仓库内部的 crate、二进制和配置目录继续使用 **Prometheus AI** 名称。

[完整中文说明](README.md) · [English](README.en.md) · [日本語](README.ja.md) · [Français](README.fr.md)

**当前为早期 Alpha，推荐从 CLI + Ollama 开始。** 其他客户端、语音、IoT 和插件的接入程度不同，部分入口为模拟实现；暂不承诺生产可用、法规合规、性能收益或全平台支持。请阅读 [功能状态](docs/project-status.md)。

## 快速开始

安装 Git、rustup（仓库固定 Rust **1.99.1**）、本机链接工具和 [Ollama](https://ollama.com/)。CLI 无需 Node.js、Bun 或 Docker。先运行 Ollama 应用或在独立终端执行 `ollama serve`，然后：

```bash
git clone https://github.com/zuixi01/aufusiding-AI.git
cd aufusiding-AI
ollama pull qwen2.5:1.5b
ollama list
cargo run --locked -p prometheus-cli -- --help
cargo run --locked -p prometheus-cli -- ask "你好，请用一句话介绍自己" --model qwen2.5:1.5b
cargo run --locked -p prometheus-cli -- chat
```

REPL 中输入 `/switch qwen2.5:1.5b` 选择模型，`/help` 查看命令，`/quit` 退出。Ollama 默认地址为 `http://localhost:11434`；无模型时可显式运行 `cargo run --locked -p prometheus-cli -- --mock ask "你好"` 演示流程，模拟回复不是真实推理。`model list` 与 `model test` 不能用于验证真实安装或连通性。

## 文档与贡献

- [中文主文档、仓库结构与桌面端入口](README.md)
- [入门、配置、HTTP 服务及常见问题](docs/getting-started.md)
- [功能状态与已知限制](docs/project-status.md)
- [贡献指南](CONTRIBUTING.md)
- [安全策略](SECURITY.md) · [行为准则](CODE_OF_CONDUCT.md)

代码使用 [MIT License](LICENSE)；外部模型与第三方服务另有条款。
