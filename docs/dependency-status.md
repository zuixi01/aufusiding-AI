# 依赖审计记录

这是 2026-11-03 的检查快照，不是持续有效的安全保证。后续变更应重新运行审计，以当前锁文件和漏洞数据库为准。

## 本轮处置

- 使用 `cargo-audit 0.22.2` 检查根目录锁文件，RustSec 数据库提交为 `263a4d951a0867d4bddc29dddd6d59427c78f9a7`（2026-11-02）。
- 将 `utoipa` 升至 6、`utoipa-swagger-ui` 升至 10.0.2，间接依赖 `zip` 从 3.0.0 升至 9.3.0，修复审计检出的四项 zip 漏洞。
- 整合两轮 Dependabot 分支，更新音频、数据库、认证、CLI、WebSocket、测试、加密及 WASM 依赖；UniFFI runtime/build/bindgen 统一到 0.32.2。
- httpmock、indicatif 和 UniFFI 升级移除了 async-std、number_prefix 和 bincode 的三条未维护警告。JWT 10 显式使用 AWS-LC 后端。
- wasmi 2 和 tabled 0.22 移除了 paste、proc-macro-error2 的两条未维护警告。
- 更新后的审计退出码为 0，`vulnerabilities` 组为 0；另有以下三条上游警告，未配置忽略项。
- npm 锁文件使用官方 registry，升级 Vite 后 `npm audit --audit-level=moderate` 返回 0 项漏洞。

Rust 锁文件 SHA256：`1d4a8b4b8fb9e168d9c250a8902af32eb19ea0c5481537c33285b9cb1bdb3e56`。锁文件变化后，此处的快照不能替代重新审计。

UniFFI 0.32.2 的生成代码使用 32 KiB 的编译期元数据暂存缓冲区，随后导出实际大小的静态字节数组。该缓冲区不能直接改为 `static`，因为 `into_array(self)` 会消耗其值。`ffi/build.rs` 仅对唯一的生成常量附加 `clippy::large_const_arrays` 的 `expect`，保留 workspace 的 `-D warnings`；声明不再匹配或告警消失时会要求重新处理。Swift/Kotlin 绑定保持同版生成器的原始输出，CI 检查重新生成结果与提交文件一致。

## 尚未消除的上游警告

| 依赖 | 类型与公告 | 引入路径 | 后续处理 |
| --- | --- | --- | --- |
| basic-toml 0.1.10 | 未维护，[RUSTSEC-2026-0374](https://rustsec.org/advisories/RUSTSEC-2026-0374.html) | UniFFI 绑定生成工具 | 随 UniFFI 升级验证生成绑定 |
| proc-macro-error 1.0.4 | 未维护，[RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) | Linux Tauri / GTK | 跟进 Tauri、GTK 依赖链 |
| glib 0.18.5 | 不健全接口（unsound），[RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) | Linux Tauri / GTK 0.18 | 修复版本为 >=0.20；需上游兼容迁移，不能直接强制替换 GTK 所需的主版本 |

`cargo audit` 默认不把上述警告计入失败，不表示问题不存在。特别是 glib 的 unsound 警告，仍须在 Linux 桌面分发前评估相关接口的可达性和上游修复方案；本轮未做该安全影响分析。桌面端继续按实验性组件提供，不承诺生产安全。

## 复查

```bash
cargo audit
cd apps/desktop
npm ci
npm audit --audit-level=moderate
```

凭据扫描使用 `.gitleaks.toml` 默认规则，仅对已核实的合成测试数据添加精确路径与完整值同时匹配的例外。它不替代完整的源代码安全审计或第三方资源权属核查。
