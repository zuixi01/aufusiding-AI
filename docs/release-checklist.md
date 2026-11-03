# 发布检查清单

当前版本处于早期开发阶段。CI 编译通过仅说明检查过的构建路径可用，不能代替真实设备上的安装、交互和升级验证。没有完成验证的平台，不应标注为稳定支持。

## 发布前

- [ ] 从干净检出开始，确认工作区没有意外文件或凭据；记录待发布 commit。
- [ ] 使用 `rust-toolchain.toml` 固定的 Rust，以及 Node.js 22（>=22.12）和 npm 10。
- [ ] 提交根目录 `Cargo.lock` 和 `apps/desktop/package-lock.json`；使用 `cargo ... --locked` 和 `npm ci`，不在发布过程中重新解析版本。
- [ ] 确认 `cargo fmt --all -- --check`、CI 中的 workspace check/test/Clippy 和 Security Audit 全部通过；依赖告警必须先评估并记录处置，不能静默忽略。
- [ ] 在 `apps/desktop` 执行 `npm ci`、`npm run build` 和 `npm audit --audit-level=moderate`。
- [ ] 核对 workspace 版本、桌面 `package.json`、`src-tauri/Cargo.toml`、`tauri.conf.json` 和预期标签版本一致；更新变更记录，列出限制和兼容性变化。
- [ ] 确认 MIT 许可证适用于贡献代码，第三方依赖、字体、图标和模型的来源及再分发授权已核实。
- [ ] 启用 GitHub private vulnerability reporting，并按 `SECURITY.md` 验证私密漏洞报告入口可用；文档链接存在不能代替设置已开启的证据。

## 功能验证

- [ ] CLI：运行 `cargo run -p prometheus-cli --locked -- --help`；使用真实配置验证目标功能，记录所需外部服务和模型版本。
- [ ] HTTP server：在本地验证健康检查、认证成功/失败、主要 API 和数据目录的读写；不要把本地冒烟测试当作生产部署验证。
- [ ] 桌面：在目标系统执行 `npm run tauri -- build -- --locked`，验证首次启动、设置、对话、错误提示和应用退出。构建前安装 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。末尾 `-- --locked` 将锁定选项传给 Cargo。
- [ ] 每个平台分别安装并测试产物。macOS 签名/公证、Windows 签名以及更新路径没有验证时，在发布说明中明确说明。
- [ ] Android/iOS 的工程与 FFI 绑定单独使用真实工具链和设备验证；目前常规 CI 没有覆盖移动端，不声明其已通过测试。

## 标签与发布

推送 `v*` 标签会触发现有的两条有外部写入行为的工作流：`Release Desktop` 创建并上传桌面草稿 Release，`Docker Build & Push` 向 GHCR 推送镜像。创建或推送标签前，维护者需要确认版本、权限和发布范围。普通分支 CI 不会执行这些发布操作。

- [ ] 所有候选产物均来自同一 commit；检查工作流结论、文件名、版本号和上传完整性。
- [ ] 桌面工作流保持 `releaseDraft: true`；平台测试完成、发布说明写清已验证范围后，再由维护者发布草稿。
- [ ] 容器镜像需要单独验证构建、健康检查、持久化和认证，再推荐用户部署；发布标签会直接推送镜像，不受桌面草稿状态控制。
- [ ] 发布说明提供安装方式、配置/数据迁移、已知问题以及上一稳定版本；发生回归时停止推荐新版本，并记录恢复方法。

本清单不代表已执行发布。仓库可见性、分支保护、私密漏洞报告和凭据扫描等 GitHub 设置由维护者在仓库设置中核对。
