# 普罗米修斯 P1 开发执行计划

## 背景

当前核心模块的算法框架已完成（186个测试通过），但所有本地推理都是 Mock 实现：
- Embedding 用 `sin()` 生成假向量
- 意图分类用关键词匹配
- 本地 LLM 推理返回 `[llama-mock]` 前缀字符串
- 反思模块用规则驱动

**目标**：通过集成 Ollama 本地推理引擎，将所有 Mock 替换为真实模型，完成核心模块完全体。

**开发方式**：严格 TDD（先写测试 → 测试失败 → 实现代码 → 测试通过）

**运行环境**：Termux (Android) / ARM64 / 7.3GB RAM / 8核 / 162GB 可用存储

---

## Phase 0：环境准备（Ollama + 模型部署）

在手机 Termux 环境中部署 Ollama 和适配的小模型。

### Task 0.1 安装 Ollama
- **操作**: 在 Termux 中安装 Ollama
  ```bash
  curl -fsSL https://ollama.com/install.sh | sh
  ```
- **验证**: `ollama --version` 输出版本号

### Task 0.2 下载对话模型
- **模型**: `qwen2.5:1.5b`（Q4 量化，约 1GB）
  - 7.3GB RAM 下最稳定的选择
  - 中文能力好，适合意图分类和对话
- **操作**: `ollama pull qwen2.5:1.5b`
- **验证**: `ollama run qwen2.5:1.5b "你好"` 返回正常回复

### Task 0.3 下载 Embedding 模型
- **模型**: `all-minilm`（约 45MB，维度 384）
  - 极轻量，适合手机
  - 英文为主，中文可用
- **备选**: `nomic-embed-text`（约 274MB，维度 768）
- **操作**: `ollama pull all-minilm`
- **验证**: `curl http://localhost:11434/api/embed -d '{"model":"all-minilm","input":"test"}'` 返回向量

### Task 0.4 验证 Ollama 服务
- **操作**:
  - `ollama serve`（后台启动）
  - `curl http://localhost:11434/api/tags` 返回已安装模型列表
  - 确认 `qwen2.5:1.5b` 和 `all-minilm` 都在列表中
- **验证**: API 可正常访问

---

## Phase 1：Ollama Provider（基础设施）

打通本地推理引擎，让系统能调用本地模型生成对话和向量。

### Task 1.1 OllamaProvider 结构体与配置
- **文件**: `core/src/providers/ollama.rs`（新建）
- **内容**: 创建 `OllamaProvider` 结构体，包含 `base_url`、`client`、`timeout`、`embed_model` 字段
- **TDD**:
  - 测试 `provider.name() == "ollama"`
  - 测试默认 `base_url` 为 `http://localhost:11434`
  - 测试 `with_base_url()` 和 `with_timeout()` 构建器
- **验证**: `cargo test providers::ollama::tests`

### Task 1.2 validate_key() — 连接检测
- **文件**: `core/src/providers/ollama.rs`
- **API**: `GET /api/tags`
- **TDD**:
  - 测试 Ollama 可达时返回 `Ok(true)`（httpmock 模拟 200）
  - 测试 Ollama 不可达时返回 `Ok(false)`（httpmock 模拟超时/500）
- **验证**: `cargo test providers::ollama::tests::validate_key`

### Task 1.3 available_models() — 动态模型列表
- **文件**: `core/src/providers/ollama.rs`
- **API**: `GET /api/tags` 解析 `models[].name`
- **TDD**:
  - 测试返回已安装模型列表 `["qwen2.5:1.5b", "all-minilm"]`
  - 测试连接失败时返回空列表
- **实现**: 使用 `Arc<RwLock<Vec<String>>>` 缓存模型列表，`validate_key()` 时刷新
- **验证**: `cargo test providers::ollama::tests::available_models`

### Task 1.4 chat() — 对话生成
- **文件**: `core/src/providers/ollama.rs`
- **API**: `POST /api/chat`（`stream: false`）
- **TDD**:
  - 测试请求格式正确（`model`, `messages`, `stream`, `options`）
  - 测试响应解析（`message.content`, `eval_count`, `prompt_eval_count`）
  - 测试非 200 状态码返回错误
  - 测试超时处理
- **数据结构**:
  ```rust
  OllamaChatRequest { model, messages, stream, options }
  OllamaChatResponse { message, model, eval_count, prompt_eval_count }
  ```
- **验证**: `cargo test providers::ollama::tests::chat`

### Task 1.5 embed() — 向量嵌入
- **文件**: `core/src/providers/ollama.rs`
- **API**: `POST /api/embed`
- **TDD**:
  - 测试返回正确维度的向量
  - 测试请求包含正确的 `model` 和 `input`
  - 测试失败时返回错误
- **数据结构**:
  ```rust
  OllamaEmbedRequest { model, input }
  OllamaEmbedResponse { embeddings: Vec<Vec<f32>> }
  ```
- **配置**: `embed_model` 字段，默认 `all-minilm`
- **验证**: `cargo test providers::ollama::tests::embed`

### Task 1.6 模块注册与导出
- **文件**:
  - `core/src/providers/mod.rs` — 添加 `pub mod ollama;`
  - `core/src/providers/registry.rs` — 注册 OllamaProvider
- **TDD**:
  - 测试 registry 包含 "ollama" provider
  - 测试通过 registry 获取 OllamaProvider
- **验证**: `cargo test providers::registry`

### Task 1.7 Ollama 集成冒烟测试
- **文件**: `core/tests/ollama_integration.rs`（新建）
- **内容**: 标记 `#[ignore]`，需要真实 Ollama 实例
- **TDD**:
  - 测试连接真实 Ollama
  - 测试真实对话生成
  - 测试真实 embedding 生成
- **验证**: `cargo test --test ollama_integration -- --ignored`（需启动 Ollama）

---

## Phase 2：EmbeddingService 抽象层

将 embedding 生成从 Memory 层解耦，支持注入不同后端。

### Task 2.1 EmbeddingService trait 定义
- **文件**: `core/src/memory/embedding.rs`（新建）
- **内容**:
  ```rust
  #[async_trait]
  pub trait EmbeddingService: Send + Sync {
      async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
      fn dimension(&self) -> usize;
  }
  ```
- **TDD**:
  - 测试 trait 可以被实现
- **验证**: `cargo test memory::embedding`

### Task 2.2 MockEmbeddingService 实现
- **文件**: `core/src/memory/embedding.rs`
- **内容**: 将现有 `sin()` 逻辑封装为 `MockEmbeddingService`
- **TDD**:
  - 测试相同输入返回相同向量
  - 测试向量维度正确
  - 测试向量已归一化
- **验证**: `cargo test memory::embedding::tests::mock`

### Task 2.3 ProviderEmbeddingService 实现
- **文件**: `core/src/memory/embedding.rs`
- **内容**: 包装 `Provider.embed()` 为 `EmbeddingService`
- **TDD**:
  - 测试调用 Provider 的 embed 方法
  - 测试维度自动检测
  - 测试错误传播
- **验证**: `cargo test memory::embedding::tests::provider`

### Task 2.4 重构 EpisodicMemory 注入 EmbeddingService
- **文件**: `core/src/memory/episodic.rs`
- **改动**:
  - `EpisodicMemory::new()` 接受 `Box<dyn EmbeddingService>`
  - 删除私有 `generate_embedding()` 方法
  - `add_episode()` 和 `search()` 使用注入的 service
- **TDD**:
  - 测试使用 MockEmbeddingService 的行为与原来一致
  - 测试使用 ProviderEmbeddingService 的行为
- **验证**: `cargo test memory::episodic`

### Task 2.5 重构 KnowledgeBase 注入 EmbeddingService
- **文件**: `core/src/memory/knowledge.rs`
- **改动**: 同 Task 2.4
- **TDD**:
  - 测试使用 MockEmbeddingService 的行为与原来一致
  - 测试文档分块 + embedding 流程
- **验证**: `cargo test memory::knowledge`

### Task 2.6 更新 memory/mod.rs 导出
- **文件**: `core/src/memory/mod.rs`
- **内容**: 添加 `pub mod embedding;` 和 re-export
- **验证**: `cargo test memory`

---

## Phase 3：意图分类升级

用本地 LLM few-shot 替换关键词匹配，大幅提升分类准确率。

### Task 3.1 LLM 意图分类器实现
- **文件**: `core/src/router/classifier.rs`
- **改动**:
  - 添加 `LlmClassifier` 结构体，接受 `Provider` 引用
  - 构造 few-shot prompt 模板（6 分类 + 示例）
  - 解析 LLM 输出为 `IntentType`
- **TDD**:
  - 测试 prompt 模板格式正确
  - 测试 LLM 输出解析（"CodeGen" → `IntentType::CodeGen`）
  - 测试解析失败时 fallback 到关键词匹配
  - 测试使用 MockProvider 的完整流程
- **验证**: `cargo test router::classifier::tests::llm`

### Task 3.2 IntentClassifier 支持双模式
- **文件**: `core/src/router/classifier.rs`
- **改动**:
  - `classify()` 方法支持 `ClassificationMode::Keyword | LLM`
  - 默认使用 LLM，失败时 fallback 到 Keyword
  - 保持现有 API 兼容
- **TDD**:
  - 测试 LLM 模式分类结果
  - 测试 fallback 机制
  - 测试现有关键词测试仍然通过
- **验证**: `cargo test router::classifier`

---

## Phase 4：能力矩阵动态化

让路由系统根据实际表现自适应调整。

### Task 4.1 ModelId 扩展 Ollama 模型
- **文件**: `core/src/router/capability.rs`
- **改动**: `ModelId` 枚举添加 `OllamaLocal(String)` 变体
- **TDD**:
  - 测试新变体的创建和比较
  - 测试能力矩阵包含 Ollama 模型
- **验证**: `cargo test router::capability`

### Task 4.2 动态能力分数更新
- **文件**: `core/src/router/capability.rs`
- **改动**:
  - `CapabilityMatrix` 添加 `update_score()` 方法
  - 基于实际推理结果（延迟、质量评分）更新能力分数
  - 使用 EMA 平滑更新
- **TDD**:
  - 测试初始分数正确
  - 测试多次更新后分数收敛
  - 测试 EMA 计算正确
- **验证**: `cargo test router::capability::tests::dynamic`

### Task 4.3 PerformanceTracker 集成
- **文件**: `core/src/router/capability.rs`
- **改动**: `PerformanceTracker` 记录每次推理的延迟和质量，自动更新能力矩阵
- **TDD**:
  - 测试记录推理结果
  - 测试能力分数随记录变化
- **验证**: `cargo test router::capability::tests::tracker`

### Task 4.4 Router 使用动态能力矩阵
- **文件**: `core/src/router/strategy.rs`
- **改动**: `Router.select_model()` 使用动态分数而非静态分数
- **TDD**:
  - 测试路由决策反映动态分数变化
  - 测试 Ollama 模型可被路由选中
- **验证**: `cargo test router::strategy`

### Task 4.5 成本计算支持 Ollama
- **文件**: `core/src/router/cost.rs`
- **改动**: 添加 Ollama 模型的成本配置（本地模型成本为 0）
- **TDD**:
  - 测试 Ollama 模型成本为 0
  - 测试成本优化策略优先选择本地模型
- **验证**: `cargo test router::cost`

---

## Phase 5：反思层 LLM 驱动

用本地 LLM 生成高质量的对话反思。

### Task 5.1 LLM 驱动的对话质量评估
- **文件**: `core/src/mind/reflection.rs`
- **改动**:
  - `ReflectionModule` 添加可选的 `Provider` 引用
  - 新增 `reflect_with_llm()` 方法
  - 构造反思 prompt，解析 LLM 输出为 `Reflection` 结构
- **TDD**:
  - 测试 prompt 构造正确
  - 测试 LLM 输出解析
  - 测试无 Provider 时 fallback 到规则驱动
- **验证**: `cargo test mind::reflection::tests::llm`

### Task 5.2 LLM 驱动的意图识别
- **文件**: `core/src/mind/reflection.rs`
- **改动**: `identify_intent()` 支持 LLM 模式
- **TDD**:
  - 测试 LLM 意图识别结果
  - 测试 fallback 到关键词匹配
- **验证**: `cargo test mind::reflection::tests::intent`

### Task 5.3 元认知循环
- **文件**: `core/src/mind/reflection.rs`
- **改动**:
  - 新增 `meta_reflect()` 方法：对反思结果再次反思
  - 生成策略调整建议
- **TDD**:
  - 测试元认知输出包含策略建议
  - 测试策略建议格式正确
- **验证**: `cargo test mind::reflection::tests::meta`

---

## Phase 6：端到端集成

### Task 6.1 Orchestrator 集成 Ollama
- **文件**: `core/src/orchestrator.rs`
- **改动**: 支持使用 OllamaProvider 作为后端
- **TDD**:
  - 测试完整对话流程（分类 → 路由 → Ollama 生成）
  - 测试 fallback 到 Mock
- **验证**: `cargo test orchestrator`

### Task 6.2 CLI 集成 Ollama
- **文件**: `cli/src/main.rs`
- **改动**:
  - 添加 `--provider ollama` 选项
  - 添加 `--ollama-url` 配置
  - 添加 `models` 子命令列出 Ollama 模型
- **TDD**:
  - 测试 CLI 参数解析
- **验证**: `cargo test --bin prometheus-cli`

### Task 6.3 记忆系统端到端测试
- **文件**: `core/tests/memory_integration.rs`（新建）
- **内容**:
  - 测试使用真实 embedding 的情景记忆存储和检索
  - 测试使用真实 embedding 的知识库检索
  - 测试 RRF 融合检索质量
- **验证**: `cargo test --test memory_integration`

### Task 6.4 完整对话流程集成测试
- **文件**: `core/tests/integration_test.rs`（更新）
- **内容**:
  - 测试 Ollama → 分类 → 路由 → 生成 → 记忆 → 反思 完整链路
  - 测试多轮对话上下文保持
- **验证**: `cargo test --test integration_test`

### Task 6.5 性能基准测试
- **文件**: `core/tests/benchmark.rs`（新建）
- **内容**:
  - Ollama chat 延迟基准
  - Embedding 生成吞吐量
  - 意图分类延迟（关键词 vs LLM）
  - 向量检索延迟
- **验证**: `cargo test --test benchmark -- --ignored`

---

## 任务依赖关系

```
Phase 1 (Ollama Provider)     ──┬──→ Phase 3 (意图分类)
                                ├──→ Phase 5 (反思层)
                                └──→ Phase 6 (集成)

Phase 2 (EmbeddingService)    ──┬──→ Phase 6 (集成)
                                │
Phase 4 (能力矩阵)             ──┘

Phase 1 + 2 + 3 + 4 + 5      ──→ Phase 6 (集成)
```

**可并行**: Phase 1、Phase 2、Phase 4 可同时开发
**串行依赖**: Phase 3/5 依赖 Phase 1；Phase 6 依赖全部

---

## 验证标准

每个 Phase 完成后：
1. `cargo test --workspace` 全部通过
2. `cargo clippy --workspace --all-targets -- -D warnings` 零错误
3. `cargo fmt --all -- --check` 格式正确
4. 提交到 GitHub，CI 通过

最终验证：
- 启动 Ollama + qwen2.5:1.5b + all-minilm
- 运行 `cargo test --test ollama_integration -- --ignored` 全部通过
- CLI 可以用本地模型完成完整对话

---

## 预计工作量

| Phase | 任务数 | 预计时间 |
|-------|--------|---------|
| Phase 0: 环境准备 | 4 | Ollama + 模型部署 |
| Phase 1: Ollama Provider | 7 | 核心基础 |
| Phase 2: EmbeddingService | 6 | 架构重构 |
| Phase 3: 意图分类 | 2 | 功能升级 |
| Phase 4: 能力矩阵 | 5 | 功能升级 |
| Phase 5: 反思层 | 3 | 功能升级 |
| Phase 6: 集成测试 | 5 | 验证收尾 |
| **总计** | **32 个任务** | |

---

## 硬件适配说明

| 项目 | 配置 |
|------|------|
| 设备 | Android 手机 (Termux) |
| 架构 | ARM64 (aarch64) |
| 内存 | 7.3GB RAM |
| 存储 | 162GB 可用 |
| CPU | 8 核 |
| 对话模型 | qwen2.5:1.5b (Q4, ~1GB) |
| Embedding 模型 | all-minilm (45MB, 384维) |
| 预计推理延迟 | 对话 3-8s/首token，Embedding <1s |
