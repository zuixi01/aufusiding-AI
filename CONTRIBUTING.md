# 贡献指南

感谢你对 Prometheus AI 的关注！我们欢迎所有形式的贡献。

## 快速开始

### 环境准备

- Rust 1.93+（[安装](https://rustup.rs)）
- Ollama（[安装](https://ollama.com)）— 运行本地模型测试时需要

### 开发流程

```
1.  Fork 本仓库
2.  创建特性分支    git checkout -b feat/amazing-feature
3.  编写代码 + 测试（覆盖率 ≥ 80%）
4.  通过全部检查    cargo test --workspace && cargo clippy --workspace -- -D warnings
5.  提交变更        git commit -m "feat: add amazing feature"
6.  推送分支        git push origin feat/amazing-feature
7.  发起 Pull Request
```

## 提交规范

使用 [Conventional Commits](https://www.conventionalcommits.org/) 格式：

| 类型 | 说明 |
|------|------|
| `feat` | 新功能 |
| `fix` | Bug 修复 |
| `refactor` | 重构（不改变行为） |
| `docs` | 文档变更 |
| `test` | 测试相关 |
| `chore` | 构建/工具链变更 |
| `perf` | 性能优化 |
| `ci` | CI/CD 变更 |

示例：`feat(agent): add roundtable consensus mechanism`

## 代码标准

- 所有代码必须通过 `cargo clippy` 零警告
- 所有代码必须通过 `cargo fmt` 格式化
- 新功能必须包含测试（目标覆盖率 ≥ 80%）
- 公开 API 必须有 rustdoc 文档注释
- 不可变优先 — 创建新对象而非修改已有对象
- 函数体 < 50 行，文件 < 800 行

## Pull Request 流程

1. 确保 PR 描述清楚说明了变更内容和原因
2. 关联相关 Issue（如有）
3. 确保 CI 全部通过（test / clippy / fmt / audit）
4. 至少需要 1 位维护者 Review 后方可合并

## Issue 指南

- 使用 Issue 模板报告 Bug 或提出功能请求
- Bug 报告请包含：复现步骤、期望行为、实际行为、环境信息
- 功能请求请包含：使用场景、期望行为、可能的实现方案

## 行为准则

参与本项目即表示你同意遵守我们的 [行为准则](CODE_OF_CONDUCT.md)。

## 许可证

贡献的代码将在 [MIT 许可证](LICENSE) 下发布。
