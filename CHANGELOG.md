# 变更记录

采用 [Keep a Changelog](https://keepachangelog.com/zh-CN/) 的组织方式。当前源码版本为 `0.1.0`，处于 Alpha；此文件不表示已经发布对应 Git tag、安装包或稳定版本。

## [Unreleased]

### 修复

- 修复 CLI 在异步运行时中执行阻塞模型探测和嵌套会话操作的问题。
- 配置文件损坏时仍可通过 `config reset` 恢复；其他命令保留解析错误且不覆盖原文件。
- 真实模型不可用时报告错误，使用显式 mock 开关进入离线演示。
- 修正仓库链接、依赖锁定、CI 平台依赖和容器构建上下文。

### 变更

- 重写上手与贡献文档，区分可验证的基础路径、实验模块和平台占位入口。
- HTTP 服务默认使用本地回环地址；Compose 默认只向宿主回环地址映射端口，TLS 代理通过可选 profile 启动。
- 移除未经核实的测试数量、性能和法规合规承诺，明确维护与发布边界。
- 补充第三方字体许可、发布清单和可复现的依赖检查。
- Rust Alpha API：`DaemonBuilder::build_with_provider` 的三个可选 LLM 参数合并为 `DaemonProviderOptions`；不带 Provider 的 `build` 调用方式保持兼容。
- 整合现有依赖升级分支，适配 CPAL 0.18、WebSocket 0.29 和 JWT 10；新增音频配置、二进制往返及令牌边界验证。
- UniFFI runtime/build/bindgen 统一到 0.31，实际编入 UDL 导出；Swift/Kotlin 绑定需由同版生成器重新生成，并与对应原生库配套使用，不能混用旧版绑定。

历史设计与进度报告保留在 `docs/`，其中的阶段计划、测试数字和完成声明不是当前版本的验收证据。实际验证结果以对应提交的 CI 和发布说明为准。

[Unreleased]: https://github.com/zuixi01/aufusiding-AI/commits/main/
