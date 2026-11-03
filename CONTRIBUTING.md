# 贡献指南

欢迎参与 aufusiding-AI。内部 crate、二进制和配置目录仍使用 Prometheus AI 名称。项目目前为早期 Alpha；小而完整、能够复现和验证的改动比未经验证的功能承诺更有价值。

## 从哪里开始

- 报告问题前，阅读 [入门指南](docs/getting-started.md) 和 [功能状态](docs/project-status.md)，区分模拟路径、占位命令与真实接入。
- 使用 [Issue 模板](https://github.com/zuixi01/aufusiding-AI/issues/new/choose) 描述可复现 Bug 或使用场景。修正文档、小型修复通常可以直接提交 PR；涉及架构、数据格式、公共 API 或新依赖时，先开 Issue 说明动机与取舍，降低重复工作。
- 安全问题按 [安全策略](SECURITY.md) 私下报告，避免在公开 Issue 中贴出可利用细节或真实密钥。
- 参与项目请遵守 [行为准则](CODE_OF_CONDUCT.md)。Issue 和 PR 可以使用中文或英文。

## 本地开发

安装 rustup、仓库固定的 Rust **1.99.1** 及本机 C/C++ 构建工具。工具链以 `rust-toolchain.toml` 为准；尚未声明最低 Rust 版本。真实模型测试另需 Ollama；普通单测和显式 mock 不应要求云端 API Key。

```bash
git clone https://github.com/<your-account>/aufusiding-AI.git
cd aufusiding-AI
git switch -c fix/short-description
cargo check --locked -p prometheus-cli
cargo run --locked -p prometheus-cli -- --mock ask "你好"
```

请先在 GitHub Fork 仓库，并将 `<your-account>` 替换为你的账户。真实 Ollama 路径、模型下载和配置见 [入门指南](docs/getting-started.md)。CLI 会写入用户数据目录；本地体验时留意现有会话与配置。

仓库分为 `core/`、`cli/`、`server/`、`ffi/`、`plugins/` 和 `apps/`。请先查找相关实现、现有测试和约定，避免为小改动引入新的架构或依赖。只构建 CLI 不需要桌面和移动端 SDK。

## 变更与验证

提交前先定义期望行为和复现方法。修复行为 Bug 时，优先补充能够在修复前失败、修复后通过的回归测试；新增行为测试应覆盖关键成功、错误和边界路径。文档、样式等低风险改动按实际需要验证，不设没有测量依据的统一覆盖率或函数行数门槛。

基础格式检查：

```bash
cargo fmt --all -- --check
```

CLI 改动的常用检查：

```bash
cargo check --locked -p prometheus-cli
cargo test --locked -p prometheus-cli
cargo clippy --locked -p prometheus-cli --all-targets -- -D warnings
```

修改其他 crate 时，将 `-p` 替换为受影响包，并运行关联测试。例如：

```bash
cargo test --locked -p prometheus-core
cargo test --locked -p prometheus-server
```

共享 API、依赖或跨 crate 改动需要按 [.github/workflows/ci.yml](.github/workflows/ci.yml) 扩大验证。不要默认 `--workspace` 在缺少 Tauri 原生依赖的环境可用。CI 是提交级别的证据；只运行单个包不能声称整个 workspace 通过。

桌面前端使用 Node.js `>=22.12.0 <23`、npm 10 和 `apps/desktop/package-lock.json`：

```bash
cd apps/desktop
npm ci
npm run build
```

原生 Tauri 构建另需平台系统依赖；移动端另需相应 SDK 和签名配置。未运行的构建、真实模型、云端、设备或平台检查应在 PR 中明确注明，不能写成通过。

## 代码与测试约定

- 使用 Rustfmt 与 Clippy；公开 API 补充说明用途、错误和约束的 rustdoc。
- 保持改动聚焦，不混入无关格式化、生成文件、重构或锁文件更新。
- 不提交密钥、`.env`、个人会话数据库、聊天导出、模型权重、证书或本机生成资源。
- 测试使用临时目录、mock 或本地模拟服务，不能覆盖贡献者的真实配置；默认测试不依赖有效云端凭据或公网服务。
- 引入原生依赖时说明支持的平台与工具链；修改公开 FFI 或共享 API 时，同步检查桥接、客户端和文档。
- 明确区分静态示例、mock、实验模块和真实能力，不新增缺乏证据的性能、节省费用或合规宣称。
- 修复已有问题时附上复现步骤与测试；发现无关失败时记录原因，避免顺手扩大范围。

## 提交与 Pull Request

建议使用 Conventional Commits，使变更便于检索：

```text
fix(cli): report unavailable Ollama instead of a simulated reply
docs: clarify local setup and feature status
test(server): cover invalid authentication input
```

一个 PR 尽量处理一个问题。请使用 PR 模板并说明：

1. 具体问题、触发条件与修改后的行为。
2. 关联 Issue（如有）和影响范围。
3. 实际运行的检查、结果，以及未运行项目和原因。
4. 可能影响兼容性、数据格式、配置或外部服务费用的变化。
5. UI 改动的截图或可复现步骤，平台改动的系统、工具链与设备信息。

提交 PR 后检查对应提交的 CI 结果，并响应维护者评审。维护者可以要求缩小范围、补充验证或调整方案；不要把配置文件存在、mock 成功或旧提交通过当作当前变更已完成验收。

## 许可证与署名

你提交的代码将按项目现有 [MIT License](LICENSE) 分发。仅贡献你有权提交的内容；复用代码、字体、图片、数据或模型资料时保留必要的许可证和来源说明。提交 PR 不要求转让著作权。
