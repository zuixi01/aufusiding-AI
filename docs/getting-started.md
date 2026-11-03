# 入门、配置与常见问题

本文以 CLI + Ollama 为主要体验路径。项目当前为早期 Alpha；[功能状态](project-status.md) 区分了可用入口、实验模块与占位功能。内部名称仍为 Prometheus AI。

## 1. 准备开发环境

- Git。
- Rust 和 Cargo：使用仓库 `rust-toolchain.toml` 固定的 **1.99.1**，包含 `rustfmt`、`clippy`。安装 rustup 后，在仓库目录执行 Cargo 命令会选择该工具链。这是当前工程基线，不是已经验证的最低 Rust 版本。
- 本机 C/C++ 编译与链接工具。Windows 需要与 Rust target 对应的 Visual Studio C++ Build Tools；macOS 需要 Xcode Command Line Tools；Linux 需要本机编译器和链接器。
- [Ollama](https://ollama.com/)：仅真实本地模型推理需要。模型权重由 Ollama 单独管理，不包含在仓库内。

仅运行 CLI 无需 Node.js、Bun、Docker、Xcode 完整 IDE、Android SDK 或外部 SQLite 安装；核心使用 bundled SQLite。桌面端和移动端另需各平台工具链。

```bash
git clone https://github.com/zuixi01/aufusiding-AI.git
cd aufusiding-AI
rustc --version
cargo run --locked -p prometheus-cli -- --help
```

使用 `--locked` 可保持依赖与已提交的 Cargo.lock 一致。首次构建需要联网下载工具链和依赖；依赖更新应单独说明，不能用移除 `--locked` 掩盖锁文件问题。

## 2. 启动本地模型

运行 Ollama 应用，或在独立终端运行：

```bash
ollama serve
```

如果已有 Ollama 服务，重复运行可能提示端口被占用。确认原服务正常即可，不必启动第二个实例。

在另一个终端下载并查看模型：

```bash
ollama pull qwen2.5:1.5b
ollama list
```

CLI 默认连接 `http://localhost:11434`，默认模型标签是 `qwen2.5:1.5b`。可通过环境变量 `PROMETHEUS_PROVIDER_OLLAMA_URL` 指向你自己的 Ollama；远程地址会让对话离开本机，请按自己的数据边界配置。

```bash
cargo run --locked -p prometheus-cli -- ask "你好，请用一句话介绍自己" --model qwen2.5:1.5b
cargo run --locked -p prometheus-cli -- ask "用一句话解释 Rust 的所有权" --model qwen2.5:1.5b --format json
```

成功时第一条命令打印模型回复；JSON 格式包含 `reply`、`route` 和 `session_id`。响应质量和耗时由模型与硬件决定。使用 `--model` 时传入 `ollama list` 中的真实标签，不要使用旧文档中的 `qwen2.5-3b-local` 等内部路由标识。

无模型时可以明确使用离线演示模式：

```bash
cargo run --locked -p prometheus-cli -- --mock ask "你好"
cargo run --locked -p prometheus-cli -- --mock chat
```

`--mock` 使用模拟 Provider 验证交互、路由和存储流程，不验证模型质量、网络连通或真实推理能力。默认真实模式不会因 Ollama 不可达而静默给出模拟回复。

## 3. 交互和会话

```bash
cargo run --locked -p prometheus-cli -- chat
```

| REPL 命令 | 用途 |
| --- | --- |
| `/help` | 查看支持的命令 |
| `/switch qwen2.5:1.5b` | 在当前进程切换模型 |
| `/new 我的会话` | 新建会话 |
| `/sessions` | 列出会话 |
| `/use <id前缀>` | 切换已有会话 |
| `/history 10` | 查看最近 10 条历史 |
| `/export session.json` | 导出当前会话 |
| `/quit` | 退出 |

也可使用非交互式会话命令：

```bash
cargo run --locked -p prometheus-cli -- session list
cargo run --locked -p prometheus-cli -- session --help
cargo run --locked -p prometheus-cli -- query-memory "Rust"
```

`ask --session <id前缀>` 可以继续已有会话；不传会话参数时会新建会话。`query-memory` 从当前数据目录的会话历史中检索关键词，并不等同于完整知识库或向量检索入口。`session delete` 会归档会话并清理其消息历史，请确认目标后再执行。会话 ID 前缀应足够长，以避免匹配歧义。

## 4. CLI 配置和本地数据

```bash
cargo run --locked -p prometheus-cli -- config list
cargo run --locked -p prometheus-cli -- config get data_dir
cargo run --locked -p prometheus-cli -- config set default_model qwen2.5:1.5b
```

以 `config list` 输出为准，默认目录通常如下：

| 平台 | 配置文件 | 数据目录 |
| --- | --- | --- |
| Windows | `%APPDATA%/prometheus-ai/config.toml` | `%APPDATA%/prometheus-ai/` |
| macOS | `~/Library/Application Support/prometheus-ai/config.toml` | `~/Library/Application Support/prometheus-ai/` |
| Linux | `$XDG_CONFIG_HOME/prometheus-ai/config.toml` 或 `~/.config/prometheus-ai/config.toml` | `$XDG_DATA_HOME/prometheus-ai/` 或 `~/.local/share/prometheus-ai/` |

主要配置的接入范围：

| 配置项 | 当前行为 |
| --- | --- |
| `default_model` | 默认模型标签；单次 `ask --model` 或 REPL `/switch` 可覆盖 |
| `history_size` | REPL 输入历史的容量设置 |
| `data_dir` | 保存会话、消息数据库和 REPL 输入历史；可通过 `config set data_dir <path>` 或 TOML 修改；相对路径取决于启动目录 |
| `routing_strategy`、`monthly_budget`、`max_tokens`、`temperature` | 可保存和读取，目前未完整连接到 CLI 推理参数或预算执行；不要依赖这些字段控制计费或生成上限 |
| `show_route_info`、`color_output` | 已有配置字段，其显示行为仍需进一步统一 |

`config reset` 会重写默认配置。修改配置前请备份已有文件，尤其是自定义 `data_dir`。SQLite 数据库、对话导出和 `repl_history.txt` 可能包含原始对话；不要提交到仓库。这里的本地存储不构成默认加密或法规合规保证。

## HTTP 服务：本机开发

HTTP 服务提供 `/api/v1` 下的接口，以及 OpenAPI 文档和指标。默认监听 `127.0.0.1:3000`，默认数据目录是当前工作目录的 `./data`，CLI 与服务端分别管理数据。未显式设置 Provider 时使用本机 Ollama；需要先完成模型下载。

Bash / zsh：

```bash
PROMETHEUS_HOST=127.0.0.1 \
PROMETHEUS_DATA_DIR=./data \
PROMETHEUS_PROVIDER_OLLAMA_URL=http://localhost:11434 \
PROMETHEUS_MODEL=qwen2.5:1.5b \
cargo run --locked -p prometheus-server
```

PowerShell：

```powershell
$env:PROMETHEUS_HOST = "127.0.0.1"
$env:PROMETHEUS_DATA_DIR = "./data"
$env:PROMETHEUS_PROVIDER_OLLAMA_URL = "http://localhost:11434"
$env:PROMETHEUS_MODEL = "qwen2.5:1.5b"
cargo run --locked -p prometheus-server
```

启动后在本机打开：

- [健康检查](http://localhost:3000/api/v1/health)
- [Swagger UI](http://localhost:3000/api/docs)
- [OpenAPI JSON](http://localhost:3000/api/openapi.json)
- [指标](http://localhost:3000/metrics)

健康检查通过只表示服务可响应；请另外验证一轮真实模型对话。无模型时可显式设置 `PROMETHEUS_MOCK_PROVIDER=1` 使用模拟 Provider，不要把模拟结果当作云端或 Ollama 已连通的证据。

| 服务端环境变量 | 作用 |
| --- | --- |
| `PROMETHEUS_HOST`、`PROMETHEUS_PORT` | 监听地址和端口；本机开发使用 loopback |
| `PROMETHEUS_DATA_DIR` | 服务端持久化数据目录 |
| `PROMETHEUS_PROVIDER_OLLAMA_URL` | Ollama 地址 |
| `PROMETHEUS_PROVIDER_OPENAI_KEY` | 注册真实 OpenAI Provider |
| `PROMETHEUS_PROVIDER_ANTHROPIC_KEY` | 注册真实 Anthropic Provider |
| `PROMETHEUS_DEFAULT_PROVIDER` | 已注册 Provider 中的首选名称，例如 `ollama`、`openai` 或 `anthropic` |
| `PROMETHEUS_MODEL` | 实际发送给 Provider 的模型名；应与首选 Provider 匹配 |
| `PROMETHEUS_MOCK_PROVIDER` | 显式启用模拟演示，设置为 `1` |
| `PROMETHEUS_AUTH_ENABLED` | `true` 或 `1` 启用认证；默认关闭，仅适用于受控本机开发 |
| `PROMETHEUS_JWT_SECRET` | JWT 签名密钥；启用认证前设置独立的强随机值 |
| `PROMETHEUS_API_KEYS` | API Key 与权限列表，格式为 `<key>:admin,<key>:write,<key>:read` |
| `PROMETHEUS_CORS_ORIGINS` | 逗号分隔的浏览器来源白名单 |
| `PROMETHEUS_LOG_FORMAT`、`RUST_LOG` | 日志格式（如 `json`）和日志过滤级别 |

启用认证后，受保护接口使用 `X-API-Key` 或 `Authorization: Bearer <token>`。请自行生成密钥，通过本机环境或部署密钥管理器注入；不要把密钥写入命令示例、Issue 或提交。API Key 格式里的权限标签不等同于已经完成的多租户隔离，部署前需审阅各接口的实际权限检查。

配置云端 Provider 会让请求可能发送给外部服务并产生费用；本地优先的项目目标不代表每一条路由都有完整预算或数据外发约束。内置默认云模型可能不适用于你的账户或供应商当前目录，请通过 `PROMETHEUS_MODEL` 设置实际可用模型。当前 fallback 不会跨 Provider 重映射模型名，混合供应商回退可能失败。请勿直接把本机开发配置暴露到公网。

## Docker Compose：本机演示

需要本机 Docker Engine/Desktop 和 Compose 插件。仓库的 [Compose 配置](../docker-compose.yml) 默认只启动 `server`，将容器的 3000 端口映射到宿主机的 `127.0.0.1:3000`，通过命名卷 `prometheus-data` 保存服务端数据。宿主端口可用 `PROMETHEUS_PORT` 修改，容器内部端口仍是 3000。Compose 与直接运行服务不同：**它固定启用认证，并要求提供 JWT 密钥**。

以下先使用显式 mock 验证容器服务、认证和持久化入口。请在仓库根目录执行；密钥只生成到当前终端环境，不要把展开后的值贴进 Issue 或提交。

Bash / zsh（需要 OpenSSL）：

```bash
export PROMETHEUS_JWT_SECRET="$(openssl rand -hex 32)"
export PROMETHEUS_API_KEYS="$(openssl rand -hex 32):admin"
export PROMETHEUS_MOCK_PROVIDER=1
docker compose config --quiet
docker compose build server
docker compose up -d server
```

PowerShell：

```powershell
$composeRandom = [Security.Cryptography.RandomNumberGenerator]::Create()
$composeBytes = New-Object byte[] 32
$composeRandom.GetBytes($composeBytes)
$env:PROMETHEUS_JWT_SECRET = [Convert]::ToBase64String($composeBytes)
$composeRandom.GetBytes($composeBytes)
$env:PROMETHEUS_API_KEYS = [Convert]::ToBase64String($composeBytes) + ":admin"
$composeRandom.Dispose()
$env:PROMETHEUS_MOCK_PROVIDER = "1"
docker compose config --quiet
docker compose build server
docker compose up -d server
```

在同一终端中检查健康和受保护的会话列表：

```bash
curl http://localhost:3000/api/v1/health
curl -H "X-API-Key: ${PROMETHEUS_API_KEYS%:admin}" http://localhost:3000/api/v1/sessions
```

PowerShell 使用：

```powershell
Invoke-RestMethod http://localhost:3000/api/v1/health
Invoke-RestMethod http://localhost:3000/api/v1/sessions -Headers @{ "X-API-Key" = $env:PROMETHEUS_API_KEYS.Split(':')[0] }
```

健康响应和会话列表不能证明真实推理已连通。切换真实 Ollama 时，将 `PROMETHEUS_MOCK_PROVIDER` 设为 `0`，设置容器可访问的 `PROMETHEUS_PROVIDER_OLLAMA_URL` 与实际 `PROMETHEUS_MODEL`，再运行 `docker compose up -d server` 更新环境。

容器内的 `localhost` 指向容器自身。访问宿主机 Ollama 时，Compose 提供 `host.docker.internal` 映射，但宿主机 Ollama 的监听地址和防火墙仍必须允许来自容器的连接；默认仅监听宿主 loopback 的服务可能无法访问。请先确认网络和访问边界，不要仅改 URL 就假定模型已可用。Compose 当前透传 JWT、API Keys、CORS、Ollama URL、模型和 mock 变量；宿主机设置的其他云端 Provider 密钥不会自动进入容器，需要单独配置。

默认本机演示不启用 nginx。`nginx` 属于可选的 `tls` profile，需要自行准备匹配 [nginx 配置](../nginx/nginx.conf) 的证书；启用后会发布宿主机的 80/443 端口，不能当作上述仅 loopback 的演示配置。TLS、公网访问、升级和恢复需要独立验收，参考 [发布检查清单](release-checklist.md)。

完成演示后可用 `docker compose stop server` 停止服务，命名卷会保留。保留本机密钥以继续使用同一配置；更换密钥或删除数据应作为明确的维护操作处理。

## 常见问题

| 现象 | 检查方法 |
| --- | --- |
| 无法连接 Ollama | 确认 Ollama 已启动、端口为 11434，检查 `PROMETHEUS_PROVIDER_OLLAMA_URL`；先用 Ollama 自身完成一次对话 |
| 提示找不到模型 | 先执行 `ollama list`；下载与 `--model` 或 `default_model` 完全一致的标签 |
| 回复出现 `mock` | 检查是否显式使用 CLI `--mock` 或服务端 `PROMETHEUS_MOCK_PROVIDER`；这是模拟路径 |
| `model list` 显示未安装模型 | 该命令目前是静态目录，不能代替 `ollama list` |
| `model info/test` 没有真实结果 | 仍为占位功能，请通过真实对话验证连通性 |
| `query-memory` 没有结果 | 确认当前 `data_dir` 下已有对话，尝试历史中的关键词；该命令搜索会话历史，不是独立知识库 |
| 编译提示缺少 linker 或 C 编译器 | 安装与当前 Rust target 匹配的本机构建工具；SQLite 和部分依赖包含本机编译步骤 |
| workspace 构建提示缺少 GTK/WebKit 等 | Tauri 需要额外系统库；先按本指南仅构建 CLI，再按平台要求配置桌面环境 |
| 修改预算或 token 配置但无效果 | 这些配置尚未完整接入推理，见上表；不能用作费用或资源硬限制 |

反馈问题请附上提交版本、操作系统、`rustc --version`、执行命令、期望行为和脱敏错误。不要上传完整环境变量、个人会话数据库或包含密钥的日志。
