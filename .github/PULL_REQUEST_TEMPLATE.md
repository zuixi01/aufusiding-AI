## 变更描述

<!-- 简要描述本次变更的内容和原因 -->

## 变更类型

- [ ] Bug 修复 (fix)
- [ ] 新功能 (feat)
- [ ] 重构 (refactor)
- [ ] 文档 (docs)
- [ ] 测试 (test)
- [ ] 性能优化 (perf)
- [ ] CI/CD (ci)
- [ ] 其他 (chore)

## 关联 Issue

<!-- 关联的 Issue 编号，例如：Closes #123 -->

## 验证记录

<!-- 列出实际执行的命令、结果和未覆盖的范围。纯文档变更可说明不适用；行为变更应验证相关场景和边界。不要把计划执行的检查写成已通过。 -->

## 检查清单

- [ ] 相关 Rust 代码通过 `cargo clippy --workspace --all-targets --locked -- -D warnings`（或说明未执行原因）
- [ ] 代码通过 `cargo fmt --all -- --check`
- [ ] 公开 API 有 rustdoc 文档注释
- [ ] 依赖变更同步更新了锁文件；桌面变更通过 `npm ci` 和 `npm run build`
- [ ] 无硬编码密钥或敏感信息
- [ ] 变更不会破坏现有 API 兼容性（或已在描述中说明）
