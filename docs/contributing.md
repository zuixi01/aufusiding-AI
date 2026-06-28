# 贡献指南

感谢你对 Prometheus 项目的关注！

## 开发环境

### 必需工具

- Rust 1.75+ (stable)
- Xcode 15+ (macOS/iOS 开发)
- SQLite 3

### 可选工具

- Android Studio (Android 开发)
- Node.js 18+ (Tauri 桌面端)

### 环境初始化

```bash
bash scripts/setup-dev.sh
```

## 代码规范

- Rust: 遵循 `rustfmt` 和 `clippy` 规范
- Swift: 遵循 Swift API Design Guidelines
- 所有公开 API 必须有文档注释
- 提交前运行 `cargo fmt` 和 `cargo clippy`

## 测试驱动开发（TDD）

- 本项目要求采用 `Red -> Green -> Refactor` 流程开发。
- 新功能必须先写失败测试，再写最小实现使测试通过。
- 行为修改必须补充对应测试，禁止仅修改实现不补测试。
- 详细流程见：`docs/engineering/tdd-protocol.md`

## 工程能力技能手册

- 高质量工程技能与参考资料见：`docs/engineering/skills-playbook.md`
- `Auto-coding` 本地技能配置见：`docs/engineering/auto-coding.md`

## 提交规范

使用 Conventional Commits：

```
feat: 新功能
fix: 修复 bug
docs: 文档更新
refactor: 代码重构
test: 测试相关
chore: 构建/工具变更
```

## Pull Request 流程

1. Fork 仓库
2. 创建功能分支 (`git checkout -b feat/my-feature`)
3. 编写代码和测试
4. 确保 CI 通过 (`cargo check && cargo test`)
5. 提交 PR 并描述变更内容

## 项目结构

请参考 [架构说明](architecture.md) 了解项目整体设计。
