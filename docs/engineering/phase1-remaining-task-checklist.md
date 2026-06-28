# Phase 1 Remaining Task Checklist

更新原则：每完成一个任务就进入下一个，严格执行 `Red -> Green -> Refactor`。

## 已完成（近期）

- [x] S2-T02 OpenAI Provider：chat/embed/validate_key + 错误处理 + 测试
- [x] S2-T03 Anthropic Provider：chat/validate_key + 测试
- [x] S2-T03 Qwen Provider：chat/embed/validate_key + 测试
- [x] S2-T04 SQLite 存储基线：Database / HistoryStore / SessionManager 持久化
- [x] S2-T04 向量检索基线：VectorStore (SQLite + cosine KNN fallback)
- [x] S2-T05 编排器持久化恢复：Orchestrator `new_with_persistence`
- [x] S2-T05 CLI/FFI 接入持久化目录并恢复会话
- [x] S2-T07 安全金库基线能力：EncryptionManager（加解密/rekey）
- [x] 工程质量清理：WorkingMemory 与 PBRIC 去除 todo 并补测
- [x] S2-T07 统一错误体系与日志脱敏基础：`core/src/error.rs`、`core/src/logging.rs`
- [x] S2-T07 日志系统完善：文件轮转初始化 `init_file_logging()`
- [x] S2-T05 存储边界收敛（阶段 1）：SessionManager 复用 `storage::Database`
- [x] S2-T05 存储边界收敛（阶段 2）：HistoryStore 复用 `storage::Database`
- [x] S2-T05 Token 使用量持久化：`storage::usage` + orchestrator 写入/聚合查询
- [x] S1/S2 文档一致性校正（阶段 1）：README 实现状态与许可证说明同步

## 进行中（按顺序自动推进）

1. [ ] 质量门禁提升（接近企业级）
   - 目标：将 core 警告压至 0，严格执行 clippy/fmt/check/test
   - 验收：`cargo clippy --workspace -D warnings` 通过

2. [ ] S1/S2 文档一致性校正（阶段 2）
   - 目标：按实际实现能力修正“高保真愿景表述”与阶段里程碑勾选
   - 验收：README 与 `docs/dev-execution-plan.md` 的状态描述无冲突
