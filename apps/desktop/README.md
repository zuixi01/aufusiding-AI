# Prometheus AI 桌面端（实验性）

这是 aufusiding-AI 的 Tauri 2 桌面客户端。仓库包含单文件 HTML/CSS/JavaScript 前端、Vite 构建配置、Tauri 应用和调用 `prometheus-core` 的 Rust 命令。

桌面端与 CLI 的接入范围不同。前端构建成功只验证静态资源打包，不能证明原生窗口、模型对话、自动安装或跨平台分发已完成验收。推荐先通过 [CLI + Ollama](../../docs/getting-started.md) 验证真实模型路径。

## 环境与启动

- 使用仓库固定的 Rust 工具链及本机 C/C++ 编译工具。
- Node.js `>=22.12.0 <23`，npm 10；项目使用 `package-lock.json`，通过 `npm ci` 安装。
- 对应平台的 Tauri 2 原生依赖，例如 Windows C++ Build Tools/WebView2、macOS 构建工具，或 Linux GTK/WebKit 开发库。CI 的 Linux 安装列表见 [工作流](../../.github/workflows/ci.yml)。
- 真实本地推理另需 Ollama。请先按平台手动安装，启动服务并下载模型；不要把桌面端自动设置当作已验证的跨平台安装器。

从仓库根目录执行：

```bash
cd apps/desktop
npm ci
npm run build
npm run tauri dev
```

`npm run build` 生成 `dist/`；当前 Tauri 配置从此目录加载资源，因此首次原生启动前需要先构建前端。仅预览前端可以使用 `npm run dev`，浏览器中的 UI 预览不包含完整 Tauri 后端能力。

## 当前限制

- 桌面端启动时探测固定的 `http://localhost:11434`；不可达时仍会回退 MockProvider。这与 CLI 的显式 `--mock` 不同，看到回复不代表真实模型已连接。
- 模型列表、默认模型和可切换 ID 仍包含硬编码的内部路由标识，如 `qwen2.5-7b-local`，不一定对应已安装的 Ollama 标签；不能沿用 CLI 的模型配置假定桌面端也已同步支持。
- 自动设置使用 `which` 检测程序，并通过 `sh` 执行安装脚本；Windows 等平台的完整流程尚未验收。建议使用各平台的 Ollama 正式安装方式，再独立检查真实对话。
- IoT 扫描包含模拟设备，语音、同步、隐私和环境智能需要逐项验证，不能仅凭界面或后端命令存在判断已可用。
- 原生启动、安装包、签名、升级与设备集成仍需要平台级测试。本轮前端构建结果不构成这些检查通过的证据。

反馈桌面端问题时，请附上操作系统、Rust/Node.js/npm 版本、启动步骤和脱敏日志，并说明复现发生在浏览器预览、Tauri 开发模式还是安装包中。更多范围见 [功能状态](../../docs/project-status.md)，贡献流程见 [贡献指南](../../CONTRIBUTING.md)。
