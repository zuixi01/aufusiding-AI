# Prometheus AI 助理 — Phase 1 精细化开发执行方案

> 版本：v1.0 | 编制日期：2026-02-28 | Phase 1 周期：6 Sprint × 2 周 = 12 周

---

## 一、总体规划概览

### 1.1 技术栈总览

| 层级 | 技术选型 |
|------|----------|
| 核心引擎 | Rust (共享跨平台逻辑) |
| iOS/macOS 前端 | SwiftUI |
| Android 前端 | Jetpack Compose |
| 桌面端 (Win/Linux) | Tauri |
| 本地推理 | llama.cpp / MLX |
| 存储 | SQLite + sqlite-vec |
| 插件沙箱 | Wasmtime (WASM) |
| FFI 桥接 | UniFFI (Rust → Swift/Kotlin) |
| 同步加密 | Signal Protocol (libsignal) |

### 1.2 Sprint 总览与里程碑

| Sprint | 周期 | 核心目标 | 里程碑 |
|--------|------|----------|--------|
| S1 | W1-W2 | Rust 核心引擎骨架 + 本地推理 MVP | 命令行可对话 |
| S2 | W3-W4 | macOS App + 云端 API + SQLite 存储 | macOS 可用原型 |
| S3 | W5-W6 | iOS App + 同步 + 人格引擎 + 安全金库 | 双端同步可用 |
| S4 | W7-W8 | 消息渠道 + 自动化工作流 + 危机感知 | 消息集成可用 |
| S5 | W9-W10 | 局域网引擎 + 设备配对 + 守护进程 | 局域网设备互联 |
| S6 | W11-W12 | 语音唤醒 + 交互设置 + 智能扫描 + 全面联调 | Phase 1 交付 |

### 1.3 团队角色假设

| 角色 | 人数 | 职责 |
|------|------|------|
| Rust 核心工程师 | 2 | 核心引擎、推理集成、同步、安全 |
| iOS/macOS 工程师 | 2 | SwiftUI 前端、MLX 集成 |
| 后端工程师 | 1 | 同步服务、云端 API 代理 |
| 全栈工程师 | 1 | 消息渠道、自动化工作流 |

### 1.4 关键路径标识

```
S1-T01 → S1-T02 → S1-T03 → S1-T05 → S2-T01 → S2-T04 → S3-T01 → S3-T03
   ↓                  ↓
S1-T04 (本地推理)   S2-T03 (SQLite)  → S3-T05 (安全金库)
                                       → S4-T01 (消息渠道)
                                       → S5-T01 (局域网引擎)
```

以 `[关键路径]` 标注的任务为阻塞性任务，延期将直接影响后续 Sprint。

---

## 二、Sprint 1（W1-W2）：Rust 核心引擎骨架 + 本地推理 MVP

> 目标：搭建 Rust 工程结构，完成模型抽象层、智能路由、对话管理核心逻辑，集成 llama.cpp 实现本地推理，命令行可完成一轮对话。

### S1-T01：Rust Workspace 工程初始化 `[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S1-T01 |
| 任务名称 | Rust Workspace 工程初始化与 CI 搭建 |
| 具体实现内容 | 1. 创建 Cargo workspace，包含 crate：`prometheus-core`、`prometheus-llm`、`prometheus-storage`、`prometheus-sync`、`prometheus-ffi`、`prometheus-cli`（调试用）<br>2. 配置 `Cargo.toml` workspace members、共享依赖版本<br>3. 建立 `crates/` 目录结构：每个 crate 独立目录<br>4. 配置 `rust-toolchain.toml`（nightly/stable 策略）<br>5. 配置 `.cargo/config.toml`（交叉编译 target：aarch64-apple-darwin, aarch64-apple-ios, aarch64-linux-android, x86_64-unknown-linux-gnu, x86_64-pc-windows-msvc）<br>6. GitHub Actions CI：lint (clippy) + test + build matrix<br>7. 配置 `deny.toml`（cargo-deny 许可证与安全审计） |
| 技术要点 | - Workspace 内 crate 间依赖用 `path` 引用<br>- 统一 `thiserror` + `anyhow` 错误处理策略<br>- 统一日志框架 `tracing` + `tracing-subscriber`<br>- 配置 `#![forbid(unsafe_code)]`（除 FFI crate 外） |
| 预估工时 | 2 人天 |
| 依赖关系 | 无 |
| 验收标准 | 1. `cargo build --workspace` 全平台编译通过<br>2. `cargo test --workspace` 通过<br>3. `cargo clippy --workspace` 零警告<br>4. CI pipeline 绿色<br>5. 各 crate 可独立编译 |

### S1-T02：模型抽象层（Model Abstraction Layer）`[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S1-T02 |
| 任务名称 | LLM Provider 统一抽象层设计与实现 |
| 具体实现内容 | 1. `crates/prometheus-llm/src/provider.rs`：定义 `LlmProvider` trait<br>&nbsp;&nbsp;- `async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>`<br>&nbsp;&nbsp;- `async fn chat_stream(&self, request: ChatRequest) -> Result<impl Stream<Item=ChatChunk>>`<br>&nbsp;&nbsp;- `fn capabilities(&self) -> ProviderCapabilities`（支持的功能：vision, function_call, json_mode 等）<br>&nbsp;&nbsp;- `fn model_info(&self) -> ModelInfo`（上下文窗口、定价、速度评级）<br>2. `crates/prometheus-llm/src/types.rs`：统一消息类型<br>&nbsp;&nbsp;- `ChatMessage { role, content: Vec<ContentPart>, metadata }`<br>&nbsp;&nbsp;- `ContentPart` 枚举：Text / Image / Audio / ToolCall / ToolResult<br>&nbsp;&nbsp;- `ChatRequest { messages, temperature, max_tokens, tools, response_format }`<br>&nbsp;&nbsp;- `ChatResponse { message, usage: TokenUsage, latency_ms }`<br>3. `crates/prometheus-llm/src/registry.rs`：Provider 注册表<br>&nbsp;&nbsp;- `ProviderRegistry` 管理所有已注册 provider<br>&nbsp;&nbsp;- 支持运行时动态注册/注销<br>4. `crates/prometheus-llm/src/error.rs`：LLM 层统一错误类型<br>&nbsp;&nbsp;- `LlmError` 枚举：RateLimit / AuthFailed / ContextOverflow / NetworkError / InferenceError |
| 技术要点 | - 使用 `async_trait` 处理 async trait<br>- Stream 使用 `futures::Stream` + `Pin<Box<dyn Stream>>`<br>- `ProviderCapabilities` 用 bitflags 实现高效能力查询<br>- 所有类型实现 `Serialize/Deserialize`（serde）便于持久化<br>- 消息类型需兼容 OpenAI / Anthropic / 本地模型三种格式 |
| 预估工时 | 3 人天 |
| 依赖关系 | S1-T01 |
| 验收标准 | 1. trait 定义完整，含完整 rustdoc 文档<br>2. 提供 `MockProvider` 实现，单元测试覆盖所有方法<br>3. 类型序列化/反序列化测试通过<br>4. `cargo doc` 生成文档无警告 |

### S1-T03：智能路由引擎（Smart Router）`[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S1-T03 |
| 任务名称 | 模型智能路由与负载均衡引擎 |
| 具体实现内容 | 1. `crates/prometheus-llm/src/router.rs`：路由核心<br>&nbsp;&nbsp;- `Router` struct：持有 `ProviderRegistry` 引用<br>&nbsp;&nbsp;- `RoutingPolicy` 枚举：CostOptimized / SpeedOptimized / QualityOptimized / Custom(Box\<dyn RoutingStrategy\>)<br>&nbsp;&nbsp;- `RoutingStrategy` trait：`fn select(&self, ctx: &RoutingContext) -> Vec<ProviderRank>`<br>&nbsp;&nbsp;- `RoutingContext`：包含 message_complexity（token 估算）、required_capabilities、user_preference、network_status<br>2. `crates/prometheus-llm/src/router_strategies.rs`：内置策略<br>&nbsp;&nbsp;- `LocalFirstStrategy`：优先本地推理，超出能力时 fallback 云端<br>&nbsp;&nbsp;- `CostAwareStrategy`：根据 token 估算选择最经济 provider<br>&nbsp;&nbsp;- `LatencyAwareStrategy`：根据历史延迟数据选择最快 provider<br>3. `crates/prometheus-llm/src/fallback.rs`：故障转移<br>&nbsp;&nbsp;- 自动重试逻辑（指数退避）<br>&nbsp;&nbsp;- Provider 健康检查（circuit breaker 模式）<br>&nbsp;&nbsp;- Fallback 链：本地 → 云端 A → 云端 B<br>4. `crates/prometheus-llm/src/token_estimator.rs`：Token 估算器<br>&nbsp;&nbsp;- 基于 tiktoken-rs 的快速 token 计数<br>&nbsp;&nbsp;- 不同模型的 token 映射 |
| 技术要点 | - Circuit breaker 使用滑动窗口统计失败率<br>- 路由决策需在 <1ms 内完成（避免成为瓶颈）<br>- Token 估算允许 ±10% 误差，换取速度<br>- 路由结果需记录日志用于后续优化 |
| 预估工时 | 4 人天 |
| 依赖关系 | S1-T02 |
| 验收标准 | 1. LocalFirst 策略：本地可用时 100% 路由到本地<br>2. Fallback 测试：Mock 主 provider 失败后自动切换备选<br>3. Circuit breaker 测试：连续 5 次失败后熔断，30s 后半开<br>4. 路由决策延迟 benchmark < 1ms<br>5. 集成测试覆盖所有策略 |

### S1-T04：llama.cpp 本地推理集成 `[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S1-T04 |
| 任务名称 | llama.cpp 绑定与本地推理 Provider 实现 |
| 具体实现内容 | 1. `crates/prometheus-llm/src/providers/local_llama.rs`：LlamaProvider<br>&nbsp;&nbsp;- 实现 `LlmProvider` trait<br>&nbsp;&nbsp;- 封装 llama-cpp-2 (Rust binding) 或自建 FFI<br>&nbsp;&nbsp;- 模型加载：支持 GGUF 格式，可配置量化级别（Q4_K_M, Q5_K_M, Q8_0）<br>&nbsp;&nbsp;- 推理参数：temperature, top_p, top_k, repeat_penalty<br>&nbsp;&nbsp;- 流式输出：逐 token 回调<br>2. `crates/prometheus-llm/src/providers/local_model_manager.rs`：模型管理<br>&nbsp;&nbsp;- 模型文件发现（扫描指定目录）<br>&nbsp;&nbsp;- 模型元数据解析（从 GGUF header 读取）<br>&nbsp;&nbsp;- 模型加载/卸载生命周期管理<br>&nbsp;&nbsp;- 内存占用估算（根据参数量和量化级别）<br>3. `crates/prometheus-llm/src/providers/local_config.rs`：推理配置<br>&nbsp;&nbsp;- GPU 层数配置（n_gpu_layers）<br>&nbsp;&nbsp;- 线程数配置（n_threads）<br>&nbsp;&nbsp;- 上下文长度配置（n_ctx）<br>&nbsp;&nbsp;- 批处理大小（n_batch）<br>4. 构建脚本 `crates/prometheus-llm/build.rs`：<br>&nbsp;&nbsp;- 条件编译 llama.cpp（Metal/CUDA/CPU）<br>&nbsp;&nbsp;- macOS：启用 Metal 加速<br>&nbsp;&nbsp;- Linux：检测 CUDA 可用性 |
| 技术要点 | - llama.cpp 通过 `cc` crate 编译为静态库链接<br>- macOS 需链接 Metal.framework 和 Accelerate.framework<br>- 模型加载是阻塞操作，需在独立线程池执行（tokio::spawn_blocking）<br>- 内存管理：模型卸载时确保完全释放 GPU 显存<br>- GGUF 解析参考 llama.cpp 的 gguf.h 结构 |
| 预估工时 | 5 人天 |
| 依赖关系 | S1-T02 |
| 验收标准 | 1. 加载 Llama-3.2-3B-Q4_K_M 模型成功<br>2. 单轮对话响应正确，流式输出逐 token 到达<br>3. macOS Metal 加速生效（对比 CPU 速度提升 >3x）<br>4. 模型卸载后内存完全释放（通过 instruments 验证）<br>5. 并发请求排队处理，不崩溃 |

### S1-T05：对话管理器（Conversation Manager）`[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S1-T05 |
| 任务名称 | 对话生命周期管理与上下文窗口控制 |
| 具体实现内容 | 1. `crates/prometheus-core/src/conversation.rs`：对话核心<br>&nbsp;&nbsp;- `Conversation` struct：id, title, messages, created_at, updated_at, metadata<br>&nbsp;&nbsp;- `ConversationManager`：创建/加载/归档/删除对话<br>&nbsp;&nbsp;- 对话状态机：Active → Paused → Archived<br>2. `crates/prometheus-core/src/context_window.rs`：上下文窗口管理<br>&nbsp;&nbsp;- `ContextWindowManager`：根据模型 max_tokens 动态裁剪<br>&nbsp;&nbsp;- 裁剪策略：保留 system prompt + 最近 N 轮 + 摘要<br>&nbsp;&nbsp;- 自动摘要触发：当消息超过窗口 70% 时生成摘要<br>&nbsp;&nbsp;- Token 预算分配：system(10%) + history(60%) + response(30%)<br>3. `crates/prometheus-core/src/orchestrator.rs`：编排器<br>&nbsp;&nbsp;- `ChatOrchestrator`：串联 ConversationManager + Router + Provider<br>&nbsp;&nbsp;- 请求流程：接收用户输入 → 构建上下文 → 路由选择 → 调用推理 → 存储结果<br>&nbsp;&nbsp;- 流式响应管道：Provider stream → 前端 stream |
| 技术要点 | - 上下文裁剪需保证语义连贯性（不能截断 tool_call/tool_result 对）<br>- 摘要生成本身需要调用 LLM，注意避免递归<br>- Orchestrator 使用 `tokio::select!` 处理取消请求<br>- 消息 ID 使用 ULIDv7 保证有序且唯一 |
| 预估工时 | 4 人天 |
| 依赖关系 | S1-T02, S1-T03 |
| 验收标准 | 1. 创建对话 → 发送消息 → 收到回复 → 消息正确存储<br>2. 上下文超出窗口时自动裁剪，不报错<br>3. 流式响应延迟 < 100ms（首 token）<br>4. 取消请求后立即停止推理<br>5. 并发 10 个对话不冲突 |

### S1-T06：CLI 调试工具

| 字段 | 内容 |
|------|------|
| 任务编号 | S1-T06 |
| 任务名称 | 命令行调试客户端 |
| 具体实现内容 | 1. `crates/prometheus-cli/src/main.rs`：CLI 入口<br>&nbsp;&nbsp;- 使用 `clap` 解析命令行参数<br>&nbsp;&nbsp;- 子命令：`chat`（交互式对话）、`model list`（列出可用模型）、`model load <path>`<br>2. `crates/prometheus-cli/src/repl.rs`：交互式 REPL<br>&nbsp;&nbsp;- 使用 `rustyline` 实现行编辑<br>&nbsp;&nbsp;- 流式输出到终端<br>&nbsp;&nbsp;- 支持 `/quit`、`/clear`、`/switch <model>` 命令 |
| 技术要点 | - CLI 仅用于开发调试，不面向最终用户<br>- 流式输出使用 `std::io::Write::flush()` 逐 token 刷新 |
| 预估工时 | 2 人天 |
| 依赖关系 | S1-T04, S1-T05 |
| 验收标准 | 1. `prometheus-cli chat` 启动后可交互对话<br>2. 流式输出逐字显示<br>3. 模型切换命令生效 |

### S1-T07：UniFFI 桥接层基础搭建

| 字段 | 内容 |
|------|------|
| 任务编号 | S1-T07 |
| 任务名称 | UniFFI 跨语言绑定基础设施 |
| 具体实现内容 | 1. `crates/prometheus-ffi/src/lib.rs`：FFI 导出层<br>&nbsp;&nbsp;- 使用 `uniffi::export` 宏导出核心接口<br>&nbsp;&nbsp;- 导出类型：`FfiChatMessage`、`FfiConversation`、`FfiModelInfo`<br>&nbsp;&nbsp;- 导出函数：`create_engine()`, `send_message()`, `list_models()`<br>2. `crates/prometheus-ffi/src/prometheus.udl`：UniFFI 接口定义<br>&nbsp;&nbsp;- 定义所有跨语言暴露的接口<br>3. `crates/prometheus-ffi/build.rs`：生成 Swift/Kotlin 绑定<br>4. 构建脚本：生成 XCFramework（供 iOS/macOS 使用） |
| 技术要点 | - UniFFI 不支持直接导出 async fn，需用回调或阻塞包装<br>- 流式响应通过 callback interface 实现<br>- XCFramework 需包含 arm64 (iOS) + arm64 (macOS) + x86_64 (macOS) 三个 slice<br>- Kotlin 绑定通过 JNA 加载 .so/.dylib |
| 预估工时 | 3 人天 |
| 依赖关系 | S1-T05 |
| 验收标准 | 1. `uniffi-bindgen` 成功生成 Swift 绑定代码<br>2. `uniffi-bindgen` 成功生成 Kotlin 绑定代码<br>3. Swift 侧可调用 `create_engine()` 并获得返回值<br>4. XCFramework 构建成功 |

---

**Sprint 1 里程碑交付物：**
- [x] Rust workspace 工程结构完整
- [x] LLM 抽象层 + 智能路由 + 对话管理可用
- [x] llama.cpp 本地推理可用（macOS Metal）
- [x] CLI 工具可完成完整对话流程
- [x] UniFFI Swift/Kotlin 绑定生成成功
- [x] CI/CD pipeline 运行正常

**Sprint 1 工时汇总：23 人天**

---

## 三、Sprint 2（W3-W4）：macOS App + 云端 API + SQLite 存储

> 目标：完成 macOS SwiftUI 应用基础框架，对接云端 LLM API，实现 SQLite 持久化存储，形成可用的 macOS 桌面原型。

### S2-T01：macOS SwiftUI 应用骨架 `[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S2-T01 |
| 任务名称 | macOS SwiftUI 应用工程搭建与核心 UI 框架 |
| 具体实现内容 | 1. Xcode 工程初始化：`PrometheusApp/`<br>&nbsp;&nbsp;- Target：macOS 14.0+，Swift 5.9+<br>&nbsp;&nbsp;- 集成 XCFramework（prometheus-ffi）via SPM local package<br>2. `PrometheusApp/App/PrometheusApp.swift`：应用入口<br>&nbsp;&nbsp;- `@main` App struct，MenuBar + Window 双模式<br>&nbsp;&nbsp;- `AppDelegate` 处理生命周期<br>3. `PrometheusApp/Core/RustBridge.swift`：Rust 引擎桥接<br>&nbsp;&nbsp;- 封装 UniFFI 生成的 Swift 代码<br>&nbsp;&nbsp;- `EngineManager` 单例：初始化/销毁引擎<br>&nbsp;&nbsp;- 异步包装：将 Rust 回调转为 Swift async/await<br>4. `PrometheusApp/Features/Chat/ChatView.swift`：对话主界面<br>&nbsp;&nbsp;- 消息列表（LazyVStack + ScrollViewReader）<br>&nbsp;&nbsp;- 输入框（TextEditor + 发送按钮）<br>&nbsp;&nbsp;- 流式消息气泡（逐字渲染）<br>5. `PrometheusApp/Features/Chat/ChatViewModel.swift`：对话 VM<br>&nbsp;&nbsp;- `@Observable` class，管理对话状态<br>&nbsp;&nbsp;- 发送消息、接收流式响应、错误处理<br>6. `PrometheusApp/Features/Sidebar/SidebarView.swift`：侧边栏<br>&nbsp;&nbsp;- 对话列表、新建对话、搜索 |
| 技术要点 | - 使用 SwiftUI Observation 框架（@Observable）替代 ObservableObject<br>- MenuBar 模式使用 `MenuBarExtra` API<br>- 流式文本渲染使用 `AttributedString` + Markdown 解析<br>- XCFramework 通过 Package.swift binaryTarget 引入 |
| 预估工时 | 5 人天 |
| 依赖关系 | S1-T07 |
| 验收标准 | 1. macOS App 启动无崩溃<br>2. Rust 引擎成功初始化（日志可见）<br>3. 对话界面可输入文字并发送<br>4. 流式响应逐字显示在气泡中<br>5. 侧边栏可新建/切换对话 |

### S2-T02：云端 API Provider — OpenAI

| 字段 | 内容 |
|------|------|
| 任务编号 | S2-T02 |
| 任务名称 | OpenAI API Provider 实现 |
| 具体实现内容 | 1. `crates/prometheus-llm/src/providers/openai.rs`：OpenAI Provider<br>&nbsp;&nbsp;- 实现 `LlmProvider` trait<br>&nbsp;&nbsp;- 支持模型：gpt-4o, gpt-4o-mini, o1, o3-mini<br>&nbsp;&nbsp;- Chat Completions API（/v1/chat/completions）<br>&nbsp;&nbsp;- 流式响应（SSE 解析）<br>&nbsp;&nbsp;- Function calling 支持<br>&nbsp;&nbsp;- Vision 支持（图片 base64/URL）<br>2. `crates/prometheus-llm/src/providers/openai_types.rs`：API 类型<br>&nbsp;&nbsp;- 请求/响应类型映射（统一类型 ↔ OpenAI 格式）<br>&nbsp;&nbsp;- SSE chunk 解析<br>3. API Key 管理：从安全存储读取（暂用环境变量） |
| 技术要点 | - HTTP 客户端使用 `reqwest` + connection pool<br>- SSE 解析使用 `eventsource-stream` crate<br>- 速率限制处理：解析 `x-ratelimit-*` headers，主动限流<br>- 错误重试：429 → 指数退避，500 → 最多重试 3 次 |
| 预估工时 | 3 人天 |
| 依赖关系 | S1-T02 |
| 验收标准 | 1. 调用 gpt-4o-mini 返回正确响应<br>2. 流式响应逐 token 到达<br>3. Function calling 请求/响应正确解析<br>4. 429 错误自动重试成功<br>5. 网络断开时返回明确错误 |

### S2-T03：云端 API Provider — Anthropic

| 字段 | 内容 |
|------|------|
| 任务编号 | S2-T03 |
| 任务名称 | Anthropic Claude API Provider 实现 |
| 具体实现内容 | 1. `crates/prometheus-llm/src/providers/anthropic.rs`：Anthropic Provider<br>&nbsp;&nbsp;- 实现 `LlmProvider` trait<br>&nbsp;&nbsp;- 支持模型：claude-sonnet-4-20250514, claude-haiku-35-20241022<br>&nbsp;&nbsp;- Messages API（/v1/messages）<br>&nbsp;&nbsp;- 流式响应（SSE，event 类型：message_start, content_block_delta, message_stop）<br>&nbsp;&nbsp;- Tool use 支持<br>&nbsp;&nbsp;- Vision 支持<br>2. `crates/prometheus-llm/src/providers/anthropic_types.rs`：API 类型<br>&nbsp;&nbsp;- Anthropic 特有的消息格式转换<br>&nbsp;&nbsp;- content block 类型处理（text, tool_use, tool_result） |
| 技术要点 | - Anthropic SSE 格式与 OpenAI 不同，需独立解析器<br>- `anthropic-version` header 必须设置<br>- system prompt 在 Anthropic API 中是顶层字段而非 message<br>- Token 计数使用 Anthropic 返回的 usage 字段 |
| 预估工时 | 3 人天 |
| 依赖关系 | S1-T02 |
| 验收标准 | 1. 调用 Claude Sonnet 返回正确响应<br>2. 流式响应正确解析所有 event 类型<br>3. Tool use 请求/响应正确<br>4. system prompt 正确传递<br>5. 与 OpenAI Provider 可通过路由无缝切换 |

### S2-T04：SQLite 存储引擎 `[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S2-T04 |
| 任务名称 | SQLite 持久化存储与向量检索基础 |
| 具体实现内容 | 1. `crates/prometheus-storage/src/database.rs`：数据库管理<br>&nbsp;&nbsp;- `Database` struct：连接池管理（r2d2 + rusqlite）<br>&nbsp;&nbsp;- 数据库初始化与迁移（使用 `refinery` 迁移框架）<br>&nbsp;&nbsp;- WAL 模式启用，PRAGMA 优化<br>2. `crates/prometheus-storage/src/migrations/`：迁移脚本<br>&nbsp;&nbsp;- `V001__initial.sql`：conversations, messages, models, settings 表<br>&nbsp;&nbsp;- `V002__vector.sql`：sqlite-vec 扩展加载，embeddings 表<br>3. `crates/prometheus-storage/src/repos/conversation_repo.rs`：对话仓库<br>&nbsp;&nbsp;- CRUD 操作：create, get_by_id, list, update, soft_delete<br>&nbsp;&nbsp;- 消息追加、分页查询<br>4. `crates/prometheus-storage/src/repos/message_repo.rs`：消息仓库<br>&nbsp;&nbsp;- 消息存储（含 metadata JSON）<br>&nbsp;&nbsp;- 按对话 ID 查询、全文搜索（FTS5）<br>5. `crates/prometheus-storage/src/repos/settings_repo.rs`：设置仓库<br>&nbsp;&nbsp;- KV 存储：API keys, 用户偏好, 模型配置<br>6. `crates/prometheus-storage/src/vector.rs`：向量存储<br>&nbsp;&nbsp;- sqlite-vec 扩展集成<br>&nbsp;&nbsp;- embedding 存储与 KNN 查询<br>&nbsp;&nbsp;- 用于后续记忆检索 |
| 技术要点 | - rusqlite 编译时启用 `bundled` feature（自带 SQLite）<br>- sqlite-vec 作为运行时扩展加载（`conn.load_extension()`）<br>- WAL 模式 + `PRAGMA synchronous=NORMAL` 平衡性能与安全<br>- FTS5 用于对话全文搜索<br>- 数据库文件位置：`~/Library/Application Support/Prometheus/data.db`（macOS） |
| 预估工时 | 4 人天 |
| 依赖关系 | S1-T01 |
| 验收标准 | 1. 数据库创建与迁移自动执行<br>2. 对话 CRUD 操作全部通过测试<br>3. 消息全文搜索返回正确结果<br>4. sqlite-vec KNN 查询返回最近邻向量<br>5. 并发读写测试通过（10 线程） |

### S2-T05：存储层集成到核心引擎

| 字段 | 内容 |
|------|------|
| 任务编号 | S2-T05 |
| 任务名称 | 对话管理器对接 SQLite 持久化 |
| 具体实现内容 | 1. `crates/prometheus-core/src/conversation.rs` 更新：<br>&nbsp;&nbsp;- `ConversationManager` 注入 `ConversationRepo` + `MessageRepo`<br>&nbsp;&nbsp;- 对话创建时自动持久化<br>&nbsp;&nbsp;- 消息发送/接收时实时写入<br>2. `crates/prometheus-core/src/orchestrator.rs` 更新：<br>&nbsp;&nbsp;- 启动时从 SQLite 恢复对话列表<br>&nbsp;&nbsp;- Token 使用量统计持久化<br>3. `crates/prometheus-ffi/src/lib.rs` 更新：<br>&nbsp;&nbsp;- 导出对话历史查询接口<br>&nbsp;&nbsp;- 导出设置读写接口 |
| 技术要点 | - 存储操作使用 `spawn_blocking` 避免阻塞 async runtime<br>- 批量消息写入使用事务提升性能<br>- 对话列表使用内存缓存 + SQLite 持久化双层架构 |
| 预估工时 | 3 人天 |
| 依赖关系 | S1-T05, S2-T04 |
| 验收标准 | 1. 应用重启后对话历史完整恢复<br>2. 消息发送后立即持久化（kill -9 后不丢失）<br>3. 对话列表加载 < 100ms（1000 条对话） |

### S2-T06：macOS App 功能完善

| 字段 | 内容 |
|------|------|
| 任务编号 | S2-T06 |
| 任务名称 | macOS 应用 UI 完善与体验优化 |
| 具体实现内容 | 1. `PrometheusApp/Features/Chat/MessageBubble.swift`：消息气泡<br>&nbsp;&nbsp;- Markdown 渲染（代码高亮、列表、链接）<br>&nbsp;&nbsp;- 代码块复制按钮<br>&nbsp;&nbsp;- 消息操作菜单（复制、重新生成、删除）<br>2. `PrometheusApp/Features/Settings/SettingsView.swift`：设置页<br>&nbsp;&nbsp;- API Key 配置（OpenAI / Anthropic）<br>&nbsp;&nbsp;- 模型选择与默认模型设置<br>&nbsp;&nbsp;- 本地模型路径配置<br>&nbsp;&nbsp;- 路由策略选择<br>3. `PrometheusApp/Features/ModelPicker/ModelPickerView.swift`：模型选择器<br>&nbsp;&nbsp;- 下拉选择当前模型<br>&nbsp;&nbsp;- 显示模型状态（在线/离线/加载中）<br>4. `PrometheusApp/Core/KeychainManager.swift`：密钥管理<br>&nbsp;&nbsp;- API Key 存储到 macOS Keychain<br>5. 全局快捷键：`⌘+Shift+P` 唤起/隐藏窗口 |
| 技术要点 | - Markdown 渲染使用 `swift-markdown-ui` 库<br>- 代码高亮使用 `Splash` 或 `Highlightr`<br>- Keychain 使用 Security.framework<br>- 全局快捷键使用 `CGEvent.tapCreate` 或 `MASShortcut` |
| 预估工时 | 4 人天 |
| 依赖关系 | S2-T01, S2-T05 |
| 验收标准 | 1. Markdown 消息正确渲染（含代码块、列表、链接）<br>2. API Key 保存到 Keychain 并可读取<br>3. 模型切换后下一条消息使用新模型<br>4. 全局快捷键可唤起/隐藏窗口<br>5. 设置修改即时生效 |

### S2-T07：错误处理与日志系统

| 字段 | 内容 |
|------|------|
| 任务编号 | S2-T07 |
| 任务名称 | 全链路错误处理与结构化日志 |
| 具体实现内容 | 1. `crates/prometheus-core/src/error.rs`：统一错误体系<br>&nbsp;&nbsp;- `PrometheusError` 顶层枚举，包含所有子模块错误<br>&nbsp;&nbsp;- 错误码体系：E1xxx(LLM), E2xxx(Storage), E3xxx(Sync), E4xxx(Security)<br>&nbsp;&nbsp;- 用户友好错误消息映射<br>2. 日志配置：<br>&nbsp;&nbsp;- `tracing` 输出到文件（按天轮转，保留 7 天）<br>&nbsp;&nbsp;- 日志级别：Release=INFO, Debug=TRACE<br>&nbsp;&nbsp;- 敏感信息过滤（API Key, 用户消息内容）<br>3. macOS 侧错误展示：<br>&nbsp;&nbsp;- Toast 通知组件<br>&nbsp;&nbsp;- 网络错误重试提示 |
| 技术要点 | - `tracing-appender` 处理日志文件轮转<br>- 敏感信息过滤使用自定义 `tracing::Layer`<br>- 错误上报预留接口（后续接入 Sentry） |
| 预估工时 | 2 人天 |
| 依赖关系 | S1-T01 |
| 验收标准 | 1. 所有错误有明确错误码和用户友好消息<br>2. 日志文件按天轮转，7 天自动清理<br>3. 日志中不包含 API Key 明文<br>4. macOS 端错误 Toast 正确显示 |

---

**Sprint 2 里程碑交付物：**
- [x] macOS App 可用原型（对话、设置、模型切换）
- [x] OpenAI + Anthropic 云端 API 对接完成
- [x] SQLite 持久化存储可用（含向量检索基础）
- [x] 本地推理 + 云端 API 可通过路由无缝切换
- [x] 全链路错误处理与日志系统

**Sprint 2 工时汇总：24 人天**

---

## 四、Sprint 3（W5-W6）：iOS App + 多设备同步 + 人格引擎 + 安全金库

> 目标：完成 iOS SwiftUI 应用，实现端到端加密的多设备同步，上线人格引擎 v1 和安全金库 v1，形成双端可用产品。

### S3-T01：iOS SwiftUI 应用 `[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S3-T01 |
| 任务名称 | iOS SwiftUI 应用开发 |
| 具体实现内容 | 1. Xcode Target 添加：iOS 17.0+<br>&nbsp;&nbsp;- 共享 SPM package（prometheus-ffi XCFramework 含 iOS slice）<br>&nbsp;&nbsp;- 共享 SwiftUI 组件（通过 `#if os(iOS)` / `#if os(macOS)` 条件编译）<br>2. `PrometheusApp/Platforms/iOS/`：iOS 专属视图<br>&nbsp;&nbsp;- `iOSChatView.swift`：适配 iOS 导航（NavigationStack）<br>&nbsp;&nbsp;- `iOSInputBar.swift`：底部输入栏（键盘适配、语音按钮预留）<br>&nbsp;&nbsp;- `iOSSettingsView.swift`：iOS 风格设置页（Form）<br>3. `PrometheusApp/Shared/`：跨平台共享<br>&nbsp;&nbsp;- `ChatViewModel.swift`（已有，复用）<br>&nbsp;&nbsp;- `MessageBubble.swift`（已有，微调 iOS 布局）<br>&nbsp;&nbsp;- `RustBridge.swift`（已有，复用）<br>4. iOS 特有功能：<br>&nbsp;&nbsp;- 键盘弹出时自动滚动到底部<br>&nbsp;&nbsp;- 下拉刷新加载历史消息<br>&nbsp;&nbsp;- Haptic feedback（发送消息时） |
| 技术要点 | - iOS 与 macOS 共享 80%+ 代码，通过条件编译区分<br>- XCFramework 需包含 arm64-apple-ios slice<br>- iOS 上本地推理受限（内存），默认路由到云端<br>- 键盘适配使用 `.scrollDismissesKeyboard(.interactively)` |
| 预估工时 | 4 人天 |
| 依赖关系 | S2-T01, S2-T06 |
| 验收标准 | 1. iOS App 在 iPhone 15 模拟器上运行无崩溃<br>2. 对话功能完整（发送、接收、流式显示）<br>3. 设置页可配置 API Key<br>4. 键盘交互流畅无遮挡<br>5. 与 macOS 共享代码比例 > 80% |

### S3-T02：iOS 本地推理适配（MLX）

| 字段 | 内容 |
|------|------|
| 任务编号 | S3-T02 |
| 任务名称 | iOS/macOS MLX 本地推理集成 |
| 具体实现内容 | 1. `PrometheusApp/Core/MLXInference.swift`：MLX 推理封装<br>&nbsp;&nbsp;- 集成 `mlx-swift` 库<br>&nbsp;&nbsp;- 模型加载（从 App Bundle 或下载目录）<br>&nbsp;&nbsp;- 推理接口：与 Rust 引擎的 LlmProvider 对齐<br>&nbsp;&nbsp;- 流式 token 生成<br>2. `PrometheusApp/Core/MLXModelManager.swift`：模型管理<br>&nbsp;&nbsp;- 模型下载（从 HuggingFace Hub）<br>&nbsp;&nbsp;- 下载进度回调<br>&nbsp;&nbsp;- 模型缓存管理<br>3. `crates/prometheus-ffi/src/lib.rs` 更新：<br>&nbsp;&nbsp;- 添加 `register_native_provider()` 接口<br>&nbsp;&nbsp;- 允许 Swift 侧注册 MLX 作为本地 Provider |
| 技术要点 | - MLX 在 Apple Silicon 上性能优于 llama.cpp<br>- iOS 上受 6GB 内存限制，仅支持 ≤3B 参数模型<br>- macOS 上可支持更大模型（取决于统一内存）<br>- MLX 模型格式与 GGUF 不同，需独立管理 |
| 预估工时 | 4 人天 |
| 依赖关系 | S3-T01, S1-T04 |
| 验收标准 | 1. macOS 上 MLX 加载 Llama-3.2-3B 成功<br>2. iOS 上 MLX 加载 Llama-3.2-1B 成功<br>3. 推理速度：macOS > 30 tokens/s，iOS > 15 tokens/s<br>4. 路由引擎可将 MLX 作为本地 Provider 使用 |

### S3-T03：多设备同步 — 同步协议与服务端 `[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S3-T03 |
| 任务名称 | 端到端加密同步协议设计与服务端实现 |
| 具体实现内容 | 1. `crates/prometheus-sync/src/protocol.rs`：同步协议<br>&nbsp;&nbsp;- CRDT 基础：使用 HLC（Hybrid Logical Clock）生成因果有序时间戳<br>&nbsp;&nbsp;- 同步单元：`SyncEnvelope { id, hlc_timestamp, device_id, encrypted_payload, signature }`<br>&nbsp;&nbsp;- 操作类型：InsertMessage / UpdateConversation / DeleteConversation / UpdateSettings<br>&nbsp;&nbsp;- 冲突解决：Last-Writer-Wins（基于 HLC）<br>2. `crates/prometheus-sync/src/crypto.rs`：端到端加密<br>&nbsp;&nbsp;- 密钥派生：用户主密码 → Argon2id → master_key<br>&nbsp;&nbsp;- 数据加密：XChaCha20-Poly1305（每条消息独立 nonce）<br>&nbsp;&nbsp;- 设备密钥交换：X25519 ECDH<br>&nbsp;&nbsp;- 密钥轮换：每 30 天自动轮换<br>3. `sync-server/`：同步中继服务（Rust + Axum）<br>&nbsp;&nbsp;- `sync-server/src/main.rs`：HTTP/WebSocket 服务<br>&nbsp;&nbsp;- `sync-server/src/handlers/sync.rs`：同步 API<br>&nbsp;&nbsp;&nbsp;&nbsp;- `POST /sync/push`：上传加密同步包<br>&nbsp;&nbsp;&nbsp;&nbsp;- `GET /sync/pull?since={hlc}`：拉取增量更新<br>&nbsp;&nbsp;&nbsp;&nbsp;- `WS /sync/realtime`：实时推送<br>&nbsp;&nbsp;- `sync-server/src/storage.rs`：服务端存储（PostgreSQL）<br>&nbsp;&nbsp;&nbsp;&nbsp;- 仅存储加密后的 blob，服务端无法解密<br>4. `crates/prometheus-sync/src/engine.rs`：同步引擎<br>&nbsp;&nbsp;- 后台同步调度（定时 + 事件触发）<br>&nbsp;&nbsp;- 增量同步：基于 HLC 水位线<br>&nbsp;&nbsp;- 离线队列：网络不可用时本地排队 |
| 技术要点 | - HLC 保证跨设备因果一致性，无需中心化时钟<br>- 服务端是"零知识"设计，仅做加密数据中继<br>- WebSocket 用于实时推送，HTTP 用于批量同步<br>- Argon2id 参数：m=64MB, t=3, p=4（防暴力破解） |
| 预估工时 | 6 人天 |
| 依赖关系 | S2-T04 |
| 验收标准 | 1. 两台设备注册后可互相发现<br>2. 设备 A 发送消息，设备 B 在 <3s 内收到<br>3. 服务端数据库中仅存储密文（人工验证不可读）<br>4. 离线 10 分钟后重新上线，数据完整同步<br>5. 冲突场景（同时编辑对话标题）正确解决 |

### S3-T04：多设备同步 — 客户端集成

| 字段 | 内容 |
|------|------|
| 任务编号 | S3-T04 |
| 任务名称 | 同步引擎客户端集成与 UI |
| 具体实现内容 | 1. `crates/prometheus-sync/src/client.rs`：同步客户端<br>&nbsp;&nbsp;- 设备注册与身份管理<br>&nbsp;&nbsp;- 同步状态机：Idle → Syncing → Conflict → Resolved<br>&nbsp;&nbsp;- 变更监听：SQLite update hook → 触发同步<br>2. `crates/prometheus-core/src/orchestrator.rs` 更新：<br>&nbsp;&nbsp;- 集成 SyncEngine，消息写入后触发同步<br>&nbsp;&nbsp;- 接收远端消息后更新本地 UI<br>3. `PrometheusApp/Features/Sync/SyncStatusView.swift`：同步状态 UI<br>&nbsp;&nbsp;- 同步状态指示器（已同步/同步中/离线）<br>&nbsp;&nbsp;- 设备管理页面（已配对设备列表）<br>4. `PrometheusApp/Features/Sync/DevicePairingView.swift`：设备配对<br>&nbsp;&nbsp;- 二维码配对流程<br>&nbsp;&nbsp;- 配对确认弹窗 |
| 技术要点 | - SQLite update hook 在写入事务提交后触发<br>- 同步状态变更通过 Rust callback → Swift @Observable 更新 UI<br>- 二维码包含：设备公钥 + 服务端地址 + 一次性配对码 |
| 预估工时 | 4 人天 |
| 依赖关系 | S3-T03, S3-T01 |
| 验收标准 | 1. macOS ↔ iOS 配对成功<br>2. 对话实时同步，延迟 < 3s<br>3. 同步状态 UI 正确反映当前状态<br>4. 设备管理页可查看/移除已配对设备 |

### S3-T05：安全金库 v1

| 字段 | 内容 |
|------|------|
| 任务编号 | S3-T05 |
| 任务名称 | 安全金库 — 敏感数据加密存储 |
| 具体实现内容 | 1. `crates/prometheus-storage/src/vault.rs`：金库核心<br>&nbsp;&nbsp;- `Vault` struct：独立加密 SQLite 数据库（SQLCipher 或应用层加密）<br>&nbsp;&nbsp;- 金库解锁：生物识别 / PIN / 主密码<br>&nbsp;&nbsp;- 数据类型：密码、API Key、私钥、敏感笔记<br>&nbsp;&nbsp;- 条目 CRUD：create, read, update, delete<br>&nbsp;&nbsp;- 自动锁定：闲置 5 分钟后自动锁定<br>2. `crates/prometheus-storage/src/vault_crypto.rs`：金库加密<br>&nbsp;&nbsp;- 字段级加密：每个条目独立密钥<br>&nbsp;&nbsp;- 密钥层级：master_key → category_key → item_key<br>&nbsp;&nbsp;- 加密算法：AES-256-GCM<br>3. `crates/prometheus-ffi/src/vault_ffi.rs`：金库 FFI 接口<br>&nbsp;&nbsp;- unlock_vault(), lock_vault(), add_item(), get_item(), list_items()<br>4. `PrometheusApp/Features/Vault/VaultView.swift`：金库 UI<br>&nbsp;&nbsp;- 解锁界面（Face ID / Touch ID / 密码）<br>&nbsp;&nbsp;- 条目列表与详情<br>&nbsp;&nbsp;- 添加/编辑条目表单 |
| 技术要点 | - 金库数据库与主数据库物理隔离（独立文件）<br>- 解锁后 master_key 仅存在于内存，不写入磁盘<br>- macOS/iOS 使用 LocalAuthentication.framework 做生物识别<br>- 内存中的密钥使用 `zeroize` crate 在 drop 时清零 |
| 预估工时 | 5 人天 |
| 依赖关系 | S2-T04, S3-T01 |
| 验收标准 | 1. 金库创建、解锁、锁定流程完整<br>2. Face ID / Touch ID 解锁成功<br>3. 条目加密存储，数据库文件中不可见明文<br>4. 闲置 5 分钟后自动锁定<br>5. 金库数据参与多设备同步（加密传输） |

### S3-T06：人格引擎 v1

| 字段 | 内容 |
|------|------|
| 任务编号 | S3-T06 |
| 任务名称 | AI 人格系统 — 性格与语气定制 |
| 具体实现内容 | 1. `crates/prometheus-core/src/persona.rs`：人格核心<br>&nbsp;&nbsp;- `Persona` struct：name, system_prompt, tone, vocabulary_level, emoji_usage, response_length_preference<br>&nbsp;&nbsp;- 预设人格：Professional（专业）、Friendly（友好）、Concise（简洁）、Creative（创意）<br>&nbsp;&nbsp;- 自定义人格：用户可编辑所有参数<br>&nbsp;&nbsp;- `PersonaManager`：人格 CRUD + 切换<br>2. `crates/prometheus-core/src/persona_prompt.rs`：Prompt 构建<br>&nbsp;&nbsp;- 根据 Persona 参数动态生成 system prompt<br>&nbsp;&nbsp;- 模板引擎：使用 `minijinja` 渲染 prompt 模板<br>&nbsp;&nbsp;- 上下文注入：将人格参数注入到每次对话的 system prompt<br>3. `crates/prometheus-core/src/orchestrator.rs` 更新：<br>&nbsp;&nbsp;- ChatOrchestrator 在构建请求时注入当前人格的 system prompt<br>4. `PrometheusApp/Features/Persona/PersonaPickerView.swift`：人格选择器<br>&nbsp;&nbsp;- 人格卡片列表<br>&nbsp;&nbsp;- 人格预览（示例对话）<br>5. `PrometheusApp/Features/Persona/PersonaEditorView.swift`：人格编辑器<br>&nbsp;&nbsp;- 参数滑块（语气、详细度、创意度）<br>&nbsp;&nbsp;- system prompt 预览 |
| 技术要点 | - system prompt 长度需控制在 500 tokens 以内（节省上下文窗口）<br>- 人格切换不影响对话历史，仅影响后续回复<br>- 预设人格不可删除，仅可修改副本 |
| 预估工时 | 3 人天 |
| 依赖关系 | S1-T05, S2-T04 |
| 验收标准 | 1. 4 种预设人格可切换，回复风格明显不同<br>2. 自定义人格创建/编辑/删除正常<br>3. 人格切换后立即生效<br>4. 人格数据持久化并参与同步 |

---

**Sprint 3 里程碑交付物：**
- [x] iOS App 可用（对话、设置、MLX 本地推理）
- [x] macOS ↔ iOS 端到端加密同步
- [x] 安全金库 v1（生物识别解锁、加密存储）
- [x] 人格引擎 v1（预设 + 自定义人格）
- [x] 双端产品可用原型

**Sprint 3 工时汇总：26 人天**

---

## 五、Sprint 4（W7-W8）：消息渠道集成 + 自动化工作流 + 危机感知

> 目标：打通 Telegram/WeChat/Slack 消息渠道，实现自动化工作流引擎，上线危机感知 v1，让 AI 助理可通过多渠道触达用户。

### S4-T01：消息渠道抽象层

| 字段 | 内容 |
|------|------|
| 任务编号 | S4-T01 |
| 任务名称 | 消息渠道统一抽象与消息总线 |
| 具体实现内容 | 1. `crates/prometheus-core/src/channel.rs`：渠道抽象<br>&nbsp;&nbsp;- `MessageChannel` trait：<br>&nbsp;&nbsp;&nbsp;&nbsp;- `async fn send(&self, msg: ChannelMessage) -> Result<()>`<br>&nbsp;&nbsp;&nbsp;&nbsp;- `async fn receive(&self) -> Result<impl Stream<Item=ChannelMessage>>`<br>&nbsp;&nbsp;&nbsp;&nbsp;- `fn channel_type(&self) -> ChannelType`<br>&nbsp;&nbsp;- `ChannelMessage` struct：sender, content, timestamp, channel_id, reply_to<br>&nbsp;&nbsp;- `ChannelType` 枚举：Telegram / WeChat / Slack / InApp<br>2. `crates/prometheus-core/src/message_bus.rs`：消息总线<br>&nbsp;&nbsp;- `MessageBus`：统一消息路由<br>&nbsp;&nbsp;- 入站消息：任意渠道 → 标准化 → 送入 ChatOrchestrator<br>&nbsp;&nbsp;- 出站消息：ChatOrchestrator 响应 → 路由到来源渠道<br>&nbsp;&nbsp;- 消息去重（基于 message_id + channel_id）<br>3. `crates/prometheus-core/src/channel_manager.rs`：渠道管理<br>&nbsp;&nbsp;- 渠道注册/注销<br>&nbsp;&nbsp;- 渠道状态监控（连接/断开/错误）<br>&nbsp;&nbsp;- 渠道配置持久化 |
| 技术要点 | - 消息总线使用 `tokio::broadcast` channel 实现发布/订阅<br>- 所有渠道消息统一转换为内部 `ChatMessage` 格式<br>- 渠道连接使用独立 tokio task，崩溃后自动重连 |
| 预估工时 | 3 人天 |
| 依赖关系 | S1-T05, S2-T04 |
| 验收标准 | 1. MessageBus 可注册多个渠道<br>2. 入站消息正确路由到 Orchestrator<br>3. 出站消息正确路由到来源渠道<br>4. 渠道断开后自动重连 |

### S4-T02：Telegram Bot 集成

| 字段 | 内容 |
|------|------|
| 任务编号 | S4-T02 |
| 任务名称 | Telegram Bot 消息渠道实现 |
| 具体实现内容 | 1. `crates/prometheus-channels/src/telegram.rs`：Telegram 渠道<br>&nbsp;&nbsp;- 实现 `MessageChannel` trait<br>&nbsp;&nbsp;- 使用 `teloxide` crate 对接 Telegram Bot API<br>&nbsp;&nbsp;- 消息接收：long polling 模式<br>&nbsp;&nbsp;- 消息发送：支持 Markdown 格式化<br>&nbsp;&nbsp;- 支持消息类型：文本、图片、文件、语音<br>2. `crates/prometheus-channels/src/telegram_auth.rs`：鉴权<br>&nbsp;&nbsp;- Bot Token 管理<br>&nbsp;&nbsp;- 用户白名单（仅允许授权用户）<br>&nbsp;&nbsp;- 首次使用时的绑定流程（验证码） |
| 技术要点 | - Telegram Bot API 使用 long polling 避免需要公网 webhook<br>- 消息长度限制 4096 字符，超长消息需分段发送<br>- 图片通过 Telegram file API 下载后转为 base64 送入 LLM |
| 预估工时 | 3 人天 |
| 依赖关系 | S4-T01 |
| 验收标准 | 1. Telegram 发送消息 → AI 回复正确<br>2. 流式响应通过编辑消息实现逐步更新<br>3. 图片消息可被 Vision 模型处理<br>4. 未授权用户被拒绝 |

### S4-T03：Slack App 集成

| 字段 | 内容 |
|------|------|
| 任务编号 | S4-T03 |
| 任务名称 | Slack App 消息渠道实现 |
| 具体实现内容 | 1. `crates/prometheus-channels/src/slack.rs`：Slack 渠道<br>&nbsp;&nbsp;- 实现 `MessageChannel` trait<br>&nbsp;&nbsp;- Slack Socket Mode（无需公网端口）<br>&nbsp;&nbsp;- 消息接收：app_mention + direct_message 事件<br>&nbsp;&nbsp;- 消息发送：Block Kit 格式化<br>&nbsp;&nbsp;- 线程回复支持（thread_ts）<br>2. `crates/prometheus-channels/src/slack_auth.rs`：OAuth 鉴权<br>&nbsp;&nbsp;- Bot Token + App Token 管理<br>&nbsp;&nbsp;- Workspace 授权流程 |
| 技术要点 | - Socket Mode 使用 WebSocket，适合本地运行场景<br>- Block Kit 用于富文本格式化（代码块、按钮等）<br>- 线程回复保持对话上下文连贯 |
| 预估工时 | 3 人天 |
| 依赖关系 | S4-T01 |
| 验收标准 | 1. @mention Bot 后收到 AI 回复<br>2. DM 对话正常工作<br>3. 代码块正确格式化显示<br>4. 线程内多轮对话上下文连贯 |

### S4-T04：WeChat 集成（受限方案）

| 字段 | 内容 |
|------|------|
| 任务编号 | S4-T04 |
| 任务名称 | 微信消息渠道集成 |
| 具体实现内容 | 1. `crates/prometheus-channels/src/wechat.rs`：微信渠道<br>&nbsp;&nbsp;- 方案 A（企业微信）：使用企业微信 API，应用消息推送<br>&nbsp;&nbsp;- 方案 B（微信公众号）：使用公众号客服消息接口<br>&nbsp;&nbsp;- 实现 `MessageChannel` trait<br>&nbsp;&nbsp;- 消息接收与回复<br>2. `crates/prometheus-channels/src/wechat_crypto.rs`：<br>&nbsp;&nbsp;- 微信消息加解密（AES-CBC）<br>&nbsp;&nbsp;- 签名验证 |
| 技术要点 | - 微信个人号无官方 API，优先支持企业微信<br>- 企业微信需要企业认证，个人开发者可用测试号<br>- 公众号客服消息有 48 小时窗口限制<br>- 需要公网回调地址（可用 ngrok 或自建隧道） |
| 预估工时 | 3 人天 |
| 依赖关系 | S4-T01 |
| 验收标准 | 1. 企业微信应用内发送消息 → AI 回复<br>2. 消息加解密正确<br>3. 签名验证通过<br>4. 超时未回复时返回友好提示 |

### S4-T05：自动化工作流引擎

| 字段 | 内容 |
|------|------|
| 任务编号 | S4-T05 |
| 任务名称 | 自动化工作流定义与执行引擎 |
| 具体实现内容 | 1. `crates/prometheus-core/src/workflow/engine.rs`：工作流引擎<br>&nbsp;&nbsp;- `Workflow` struct：name, triggers, steps, enabled<br>&nbsp;&nbsp;- `WorkflowEngine`：工作流注册、触发、执行<br>&nbsp;&nbsp;- 执行模型：DAG（有向无环图）步骤编排<br>2. `crates/prometheus-core/src/workflow/trigger.rs`：触发器<br>&nbsp;&nbsp;- `Trigger` 枚举：<br>&nbsp;&nbsp;&nbsp;&nbsp;- Schedule(cron_expr)：定时触发<br>&nbsp;&nbsp;&nbsp;&nbsp;- Event(event_type)：事件触发（消息到达、文件变更等）<br>&nbsp;&nbsp;&nbsp;&nbsp;- Manual：手动触发<br>&nbsp;&nbsp;&nbsp;&nbsp;- Webhook(path)：HTTP webhook 触发<br>3. `crates/prometheus-core/src/workflow/step.rs`：步骤定义<br>&nbsp;&nbsp;- `Step` 枚举：<br>&nbsp;&nbsp;&nbsp;&nbsp;- LlmCall { prompt_template, model_preference }：调用 LLM<br>&nbsp;&nbsp;&nbsp;&nbsp;- HttpRequest { url, method, headers, body_template }：HTTP 请求<br>&nbsp;&nbsp;&nbsp;&nbsp;- Transform { jq_expression }：数据转换<br>&nbsp;&nbsp;&nbsp;&nbsp;- Condition { expression, if_true, if_false }：条件分支<br>&nbsp;&nbsp;&nbsp;&nbsp;- SendMessage { channel, template }：发送消息<br>&nbsp;&nbsp;&nbsp;&nbsp;- VaultRead { key }：读取金库数据<br>4. `crates/prometheus-core/src/workflow/context.rs`：执行上下文<br>&nbsp;&nbsp;- 步骤间数据传递（JSON 变量）<br>&nbsp;&nbsp;- 模板渲染（Handlebars 语法）<br>5. `crates/prometheus-core/src/workflow/scheduler.rs`：调度器<br>&nbsp;&nbsp;- Cron 调度（使用 `cron` crate）<br>&nbsp;&nbsp;- 执行历史记录<br>&nbsp;&nbsp;- 失败重试策略 |
| 技术要点 | - 工作流定义使用 YAML/JSON 格式，便于导入导出<br>- DAG 执行使用 `petgraph` crate 做拓扑排序<br>- 步骤超时控制：每步默认 30s，可配置<br>- 工作流执行在独立 tokio task 中，不阻塞主线程 |
| 预估工时 | 5 人天 |
| 依赖关系 | S1-T05, S4-T01 |
| 验收标准 | 1. 定义一个"每日摘要"工作流：定时触发 → LLM 生成摘要 → 发送到 Telegram<br>2. 条件分支正确执行<br>3. 步骤间数据传递正确<br>4. 失败步骤自动重试<br>5. 执行历史可查询 |

### S4-T06：工作流 UI

| 字段 | 内容 |
|------|------|
| 任务编号 | S4-T06 |
| 任务名称 | 工作流管理界面 |
| 具体实现内容 | 1. `PrometheusApp/Features/Workflow/WorkflowListView.swift`：工作流列表<br>&nbsp;&nbsp;- 已创建工作流列表<br>&nbsp;&nbsp;- 启用/禁用开关<br>&nbsp;&nbsp;- 最近执行状态<br>2. `PrometheusApp/Features/Workflow/WorkflowEditorView.swift`：工作流编辑器<br>&nbsp;&nbsp;- 可视化步骤编排（简化版，非拖拽）<br>&nbsp;&nbsp;- 触发器配置<br>&nbsp;&nbsp;- 步骤参数配置表单<br>3. `PrometheusApp/Features/Workflow/WorkflowHistoryView.swift`：执行历史<br>&nbsp;&nbsp;- 执行记录列表<br>&nbsp;&nbsp;- 每步执行详情（输入/输出/耗时/状态） |
| 技术要点 | - 编辑器 v1 使用表单模式，v2 再做可视化拖拽<br>- 执行历史使用分页加载 |
| 预估工时 | 3 人天 |
| 依赖关系 | S4-T05, S2-T01 |
| 验收标准 | 1. 可通过 UI 创建/编辑/删除工作流<br>2. 启用/禁用即时生效<br>3. 执行历史正确显示 |

### S4-T07：危机感知 v1

| 字段 | 内容 |
|------|------|
| 任务编号 | S4-T07 |
| 任务名称 | 对话内容危机检测与预警系统 |
| 具体实现内容 | 1. `crates/prometheus-core/src/safety/crisis_detector.rs`：危机检测器<br>&nbsp;&nbsp;- `CrisisDetector`：分析用户消息内容<br>&nbsp;&nbsp;- 检测维度：<br>&nbsp;&nbsp;&nbsp;&nbsp;- 自伤/自杀倾向关键词匹配（多语言词库）<br>&nbsp;&nbsp;&nbsp;&nbsp;- 情绪急剧变化检测（基于历史对话情绪曲线）<br>&nbsp;&nbsp;&nbsp;&nbsp;- 紧急求助信号识别<br>&nbsp;&nbsp;- 风险等级：Low / Medium / High / Critical<br>2. `crates/prometheus-core/src/safety/crisis_response.rs`：危机响应<br>&nbsp;&nbsp;- Low：记录日志，不干预<br>&nbsp;&nbsp;- Medium：AI 回复中温和引导，建议专业帮助<br>&nbsp;&nbsp;- High：弹出紧急资源卡片（心理热线、急救电话）<br>&nbsp;&nbsp;- Critical：主动推送紧急联系人（需用户预先配置）<br>3. `crates/prometheus-core/src/safety/keyword_db.rs`：关键词库<br>&nbsp;&nbsp;- 多语言危机关键词（中/英/日）<br>&nbsp;&nbsp;- 正则模式匹配<br>&nbsp;&nbsp;- 上下文消歧（避免误报）<br>4. `PrometheusApp/Features/Safety/CrisisAlertView.swift`：危机弹窗<br>&nbsp;&nbsp;- 紧急资源卡片 UI<br>&nbsp;&nbsp;- 一键拨打热线按钮<br>5. `PrometheusApp/Features/Safety/EmergencyContactView.swift`：紧急联系人<br>&nbsp;&nbsp;- 紧急联系人配置<br>&nbsp;&nbsp;- 通知方式选择（短信/电话/App 推送） |
| 技术要点 | - 关键词匹配在本地执行，不上传云端（隐私保护）<br>- 情绪检测使用简单的情感分析（基于词典，非 LLM）<br>- 误报率控制：需要多个信号同时触发才升级风险等级<br>- Critical 级别通知需用户明确授权 |
| 预估工时 | 4 人天 |
| 依赖关系 | S1-T05, S2-T04 |
| 验收标准 | 1. 包含危机关键词的消息被正确检测<br>2. 不同风险等级触发对应响应<br>3. 紧急资源卡片正确显示本地化热线号码<br>4. 正常对话不触发误报（测试 100 条正常对话，误报率 < 1%）<br>5. 紧急联系人通知功能可用 |

---

**Sprint 4 里程碑交付物：**
- [x] Telegram / Slack / 企业微信消息渠道可用
- [x] 自动化工作流引擎可用（定时/事件触发 + 多步骤编排）
- [x] 危机感知 v1 上线（关键词检测 + 分级响应）
- [x] 消息渠道管理 UI
- [x] 工作流管理 UI

**Sprint 4 工时汇总：24 人天**

---

## 六、Sprint 5（W9-W10）：局域网引擎 + 设备配对 + 守护进程 + 自主权限

> 目标：实现局域网设备发现与配对，搭建后台守护进程，完成自主权限体系，让 AI 助理具备局域网内设备互联能力。

### S5-T01：局域网设备发现引擎 `[关键路径]`

| 字段 | 内容 |
|------|------|
| 任务编号 | S5-T01 |
| 任务名称 | 局域网设备自动发现与广播引擎 |
| 具体实现内容 | 1. `crates/prometheus-network/src/discovery.rs`：设备发现核心<br>&nbsp;&nbsp;- mDNS/DNS-SD 服务发现（使用 `mdns-sd` crate）<br>&nbsp;&nbsp;- 服务注册：`_prometheus._tcp.local`<br>&nbsp;&nbsp;- 服务浏览：持续监听局域网内 Prometheus 实例<br>&nbsp;&nbsp;- 设备信息广播：device_id, device_name, device_type, capabilities, version<br>2. `crates/prometheus-network/src/device.rs`：设备模型<br>&nbsp;&nbsp;- `Device` struct：id, name, type(Mac/iPhone/iPad/Android/PC), ip, port, last_seen, paired, capabilities<br>&nbsp;&nbsp;- `DeviceCapabilities`：bitflags（can_infer_local, has_gpu, storage_available_gb）<br>&nbsp;&nbsp;- `DeviceManager`：设备列表管理、在线状态追踪<br>3. `crates/prometheus-network/src/heartbeat.rs`：心跳机制<br>&nbsp;&nbsp;- UDP 心跳包：每 5s 发送一次<br>&nbsp;&nbsp;- 超时判定：15s 无心跳标记为离线<br>&nbsp;&nbsp;- 心跳包含：device_id, timestamp, load_status |
| 技术要点 | - mDNS 在 macOS/iOS 上使用系统原生 Bonjour<br>- Linux/Android 使用纯 Rust mDNS 实现<br>- 心跳使用 UDP 广播减少连接开销<br>- 设备发现结果缓存 + 去重（同一设备多网卡场景） |
| 预估工时 | 4 人天 |
| 依赖关系 | S1-T01 |
| 验收标准 | 1. 同一局域网内两台设备互相发现 < 5s<br>2. 设备离线后 15s 内标记为离线<br>3. 设备信息（名称、类型、能力）正确广播<br>4. 多网卡环境下不重复发现 |

### S5-T02：双门安全配对

| 字段 | 内容 |
|------|------|
| 任务编号 | S5-T02 |
| 任务名称 | 设备安全配对 — 双向确认机制 |
| 具体实现内容 | 1. `crates/prometheus-network/src/pairing.rs`：配对协议<br>&nbsp;&nbsp;- 配对流程（双门机制）：<br>&nbsp;&nbsp;&nbsp;&nbsp;1. 设备 A 发起配对请求（含 A 的临时公钥）<br>&nbsp;&nbsp;&nbsp;&nbsp;2. 设备 B 收到请求，显示确认弹窗（第一道门）<br>&nbsp;&nbsp;&nbsp;&nbsp;3. 设备 B 确认后，返回 B 的临时公钥 + 6 位验证码<br>&nbsp;&nbsp;&nbsp;&nbsp;4. 设备 A 显示验证码，用户在 B 上输入确认（第二道门）<br>&nbsp;&nbsp;&nbsp;&nbsp;5. 双方使用 X25519 ECDH 协商会话密钥<br>&nbsp;&nbsp;&nbsp;&nbsp;6. 配对信息持久化<br>&nbsp;&nbsp;- `PairingManager`：配对状态机管理<br>2. `crates/prometheus-network/src/pairing_crypto.rs`：配对加密<br>&nbsp;&nbsp;- X25519 密钥对生成<br>&nbsp;&nbsp;- ECDH 密钥协商<br>&nbsp;&nbsp;- 会话密钥派生（HKDF-SHA256）<br>&nbsp;&nbsp;- 验证码生成（TOTP 算法变体）<br>3. `crates/prometheus-network/src/secure_channel.rs`：安全通道<br>&nbsp;&nbsp;- 配对后的加密通信通道<br>&nbsp;&nbsp;- 使用协商的会话密钥加密所有通信<br>&nbsp;&nbsp;- 消息认证（HMAC-SHA256）<br>4. `PrometheusApp/Features/Network/PairingView.swift`：配对 UI<br>&nbsp;&nbsp;- 发现设备列表<br>&nbsp;&nbsp;- 配对请求弹窗<br>&nbsp;&nbsp;- 验证码输入/显示界面<br>&nbsp;&nbsp;- 配对成功动画 |
| 技术要点 | - 双门机制防止中间人攻击（MITM）<br>- 验证码有效期 60s，过期需重新发起<br>- 配对信息存储在安全金库中<br>- 配对后的通信全程加密 |
| 预估工时 | 5 人天 |
| 依赖关系 | S5-T01, S3-T05 |
| 验收标准 | 1. 完整配对流程 < 30s<br>2. 双方均需确认才能配对成功<br>3. 验证码不匹配时配对失败<br>4. 配对后通信加密（抓包验证不可读）<br>5. 已配对设备重启后自动重连 |

### S5-T03：设备自动识别

| 字段 | 内容 |
|------|------|
| 任务编号 | S5-T03 |
| 任务名称 | 设备类型与能力自动识别 |
| 具体实现内容 | 1. `crates/prometheus-network/src/device_probe.rs`：设备探测<br>&nbsp;&nbsp;- 操作系统识别：macOS / iOS / Android / Windows / Linux<br>&nbsp;&nbsp;- 硬件能力探测：<br>&nbsp;&nbsp;&nbsp;&nbsp;- CPU 核心数与架构（ARM/x86）<br>&nbsp;&nbsp;&nbsp;&nbsp;- 可用内存<br>&nbsp;&nbsp;&nbsp;&nbsp;- GPU 类型与显存（Metal/CUDA/Vulkan）<br>&nbsp;&nbsp;&nbsp;&nbsp;- 磁盘可用空间<br>&nbsp;&nbsp;- 推理能力评估：根据硬件自动判断可运行的最大模型<br>2. `crates/prometheus-network/src/capability_negotiation.rs`：能力协商<br>&nbsp;&nbsp;- 配对后交换详细能力信息<br>&nbsp;&nbsp;- 推理任务分配建议（哪台设备跑推理最优）<br>&nbsp;&nbsp;- 存储容量协商（哪台设备存储模型文件）<br>3. `PrometheusApp/Features/Network/DeviceDetailView.swift`：设备详情<br>&nbsp;&nbsp;- 设备硬件信息展示<br>&nbsp;&nbsp;- 推理能力评级（星级）<br>&nbsp;&nbsp;- 当前负载状态 |
| 技术要点 | - macOS/iOS 使用 `sysctl` + `IOKit` 获取硬件信息<br>- Rust 侧使用 `sysinfo` crate 跨平台获取系统信息<br>- GPU 检测：macOS 用 Metal API，Linux 用 `nvidia-smi` 解析<br>- 推理能力评估公式：score = gpu_memory × 0.6 + ram × 0.3 + cpu_cores × 0.1 |
| 预估工时 | 3 人天 |
| 依赖关系 | S5-T01, S5-T02 |
| 验收标准 | 1. 正确识别 macOS/iOS 设备类型<br>2. 硬件信息准确（与系统信息对比）<br>3. 推理能力评估合理（M1 Mac > iPhone 15）<br>4. 设备详情页信息完整 |

### S5-T04：后台常驻守护进程

| 字段 | 内容 |
|------|------|
| 任务编号 | S5-T04 |
| 任务名称 | 后台守护进程 — 系统级常驻服务 |
| 具体实现内容 | 1. `crates/prometheus-daemon/src/main.rs`：守护进程入口<br>&nbsp;&nbsp;- 独立二进制：`prometheus-daemon`<br>&nbsp;&nbsp;- 功能：局域网发现、心跳、同步、工作流调度、消息渠道监听<br>&nbsp;&nbsp;- 进程管理：PID 文件、单实例锁<br>&nbsp;&nbsp;- 优雅关闭：SIGTERM 处理<br>2. `crates/prometheus-daemon/src/service.rs`：服务管理<br>&nbsp;&nbsp;- macOS：`launchd` plist 生成与安装<br>&nbsp;&nbsp;&nbsp;&nbsp;- `~/Library/LaunchAgents/com.prometheus.daemon.plist`<br>&nbsp;&nbsp;&nbsp;&nbsp;- KeepAlive=true, RunAtLoad=true<br>&nbsp;&nbsp;- Linux：systemd unit 文件生成<br>&nbsp;&nbsp;- 服务安装/卸载/状态查询命令<br>3. `crates/prometheus-daemon/src/ipc.rs`：进程间通信<br>&nbsp;&nbsp;- Unix Domain Socket（macOS/Linux）<br>&nbsp;&nbsp;- 协议：JSON-RPC over UDS<br>&nbsp;&nbsp;- 前端 App ↔ Daemon 通信接口<br>4. `crates/prometheus-daemon/src/resource_monitor.rs`：资源监控<br>&nbsp;&nbsp;- CPU/内存使用率监控<br>&nbsp;&nbsp;- 自适应资源占用（空闲时降低心跳频率）<br>&nbsp;&nbsp;- 低电量模式（iOS/MacBook）：暂停非关键任务 |
| 技术要点 | - 守护进程不含 UI，纯后台运行<br>- macOS launchd 比 cron 更可靠（系统级进程管理）<br>- UDS 比 TCP 更安全（无网络暴露）且更快<br>- 资源监控使用 `sysinfo` crate，每 30s 采样一次 |
| 预估工时 | 5 人天 |
| 依赖关系 | S5-T01, S4-T01, S3-T03 |
| 验收标准 | 1. `prometheus-daemon` 可作为 launchd 服务启动<br>2. 系统重启后自动启动<br>3. App 通过 UDS 与 Daemon 正常通信<br>4. Daemon CPU 占用 < 1%（空闲时）<br>5. 优雅关闭：所有任务完成后退出 |

### S5-T05：自主权限体系

| 字段 | 内容 |
|------|------|
| 任务编号 | S5-T05 |
| 任务名称 | AI 助理自主操作权限管理系统 |
| 具体实现内容 | 1. `crates/prometheus-core/src/permissions/model.rs`：权限模型<br>&nbsp;&nbsp;- `Permission` 枚举：<br>&nbsp;&nbsp;&nbsp;&nbsp;- ReadMessages：读取对话历史<br>&nbsp;&nbsp;&nbsp;&nbsp;- SendMessages：通过渠道发送消息<br>&nbsp;&nbsp;&nbsp;&nbsp;- AccessVault：访问安全金库<br>&nbsp;&nbsp;&nbsp;&nbsp;- ExecuteWorkflow：执行自动化工作流<br>&nbsp;&nbsp;&nbsp;&nbsp;- NetworkAccess：发起网络请求<br>&nbsp;&nbsp;&nbsp;&nbsp;- FileSystemRead / FileSystemWrite：文件系统访问<br>&nbsp;&nbsp;&nbsp;&nbsp;- DeviceControl：控制局域网设备<br>&nbsp;&nbsp;- `PermissionLevel` 枚举：AlwaysAllow / AskEveryTime / AllowOnce / Deny<br>2. `crates/prometheus-core/src/permissions/manager.rs`：权限管理器<br>&nbsp;&nbsp;- `PermissionManager`：权限检查与授权<br>&nbsp;&nbsp;- 权限请求流程：AI 请求权限 → 弹窗询问用户 → 记录决策<br>&nbsp;&nbsp;- 权限审计日志：所有权限请求与决策记录<br>3. `crates/prometheus-core/src/permissions/policy.rs`：权限策略<br>&nbsp;&nbsp;- 默认策略：保守模式（大部分权限默认 AskEveryTime）<br>&nbsp;&nbsp;- 用户自定义策略<br>&nbsp;&nbsp;- 时间限制：临时授权（1 小时/1 天/永久）<br>4. `PrometheusApp/Features/Permissions/PermissionRequestView.swift`：权限弹窗<br>&nbsp;&nbsp;- 权限请求说明（AI 为什么需要这个权限）<br>&nbsp;&nbsp;- 允许/拒绝/仅此一次 按钮<br>&nbsp;&nbsp;- 权限管理页面（查看/修改所有权限） |
| 技术要点 | - 权限检查在 Orchestrator 层拦截，所有 AI 操作必须过权限检查<br>- 权限弹窗通过 FFI callback 从 Rust 触发 Swift UI<br>- 审计日志不可篡改（append-only + 签名） |
| 预估工时 | 4 人天 |
| 依赖关系 | S1-T05, S2-T04 |
| 验收标准 | 1. AI 执行敏感操作时弹出权限请求<br>2. 用户拒绝后 AI 操作被阻止<br>3. "仅此一次"授权下次重新询问<br>4. 权限审计日志完整记录<br>5. 权限管理页可查看/修改所有权限设置 |

### S5-T06：局域网设备互联集成测试

| 字段 | 内容 |
|------|------|
| 任务编号 | S5-T06 |
| 任务名称 | 局域网功能端到端集成测试 |
| 具体实现内容 | 1. 集成测试场景：<br>&nbsp;&nbsp;- 场景 1：Mac 发现 iPhone，配对，同步对话<br>&nbsp;&nbsp;- 场景 2：Mac 作为推理节点，iPhone 远程调用<br>&nbsp;&nbsp;- 场景 3：设备离线后重新上线，状态恢复<br>&nbsp;&nbsp;- 场景 4：三台设备同时在线，消息广播<br>2. 测试工具：<br>&nbsp;&nbsp;- 模拟设备工具（在单机上模拟多设备）<br>&nbsp;&nbsp;- 网络延迟注入（模拟弱网环境） |
| 技术要点 | - 使用 Docker 容器模拟多设备环境<br>- 网络模拟使用 `tc`（traffic control）注入延迟/丢包 |
| 预估工时 | 2 人天 |
| 依赖关系 | S5-T01 ~ S5-T05 |
| 验收标准 | 1. 所有 4 个集成测试场景通过<br>2. 弱网环境（200ms 延迟, 5% 丢包）下功能正常<br>3. 无内存泄漏（长时间运行 1 小时） |

---

**Sprint 5 里程碑交付物：**
- [x] 局域网设备发现与自动识别
- [x] 双门安全配对机制
- [x] 后台守护进程（launchd/systemd）
- [x] 自主权限体系
- [x] 局域网设备互联端到端可用

**Sprint 5 工时汇总：23 人天**

---

## 七、Sprint 6（W11-W12）：语音唤醒 + 交互设置 + 智能扫描 + 全面联调

> 目标：完成语音唤醒引擎、交互方式设置、智能扫描入库功能，进行全面联调与性能优化，交付 Phase 1 完整产品。

### S6-T01：语音唤醒引擎

| 字段 | 内容 |
|------|------|
| 任务编号 | S6-T01 |
| 任务名称 | 本地语音唤醒词检测引擎 |
| 具体实现内容 | 1. `crates/prometheus-voice/src/wake_word.rs`：唤醒词检测<br>&nbsp;&nbsp;- 集成 `rustpotter`（Rust 原生唤醒词引擎）或 `Porcupine` SDK<br>&nbsp;&nbsp;- 默认唤醒词："Hey Prometheus" / "小普"<br>&nbsp;&nbsp;- 自定义唤醒词训练接口<br>&nbsp;&nbsp;- 检测灵敏度可调（0.0 ~ 1.0）<br>2. `crates/prometheus-voice/src/audio_capture.rs`：音频采集<br>&nbsp;&nbsp;- 使用 `cpal` crate 跨平台音频输入<br>&nbsp;&nbsp;- 音频格式：16kHz, 16bit, mono<br>&nbsp;&nbsp;- VAD（Voice Activity Detection）：静音检测，仅在有语音时处理<br>&nbsp;&nbsp;- 环形缓冲区：保留最近 3s 音频<br>3. `crates/prometheus-voice/src/stt.rs`：语音转文字（预留）<br>&nbsp;&nbsp;- `SpeechToText` trait 定义<br>&nbsp;&nbsp;- Whisper.cpp 本地 STT 集成（基础版）<br>&nbsp;&nbsp;- 云端 STT 接口预留（OpenAI Whisper API）<br>4. `crates/prometheus-voice/src/pipeline.rs`：语音管道<br>&nbsp;&nbsp;- 完整流程：音频采集 → VAD → 唤醒词检测 → STT → 文本送入 Orchestrator<br>&nbsp;&nbsp;- 状态机：Listening → WakeWordDetected → Recording → Processing |
| 技术要点 | - 唤醒词检测必须在本地运行（隐私 + 延迟）<br>- VAD 使用 `webrtc-vad` 或 `silero-vad`（ONNX 模型）<br>- 音频采集在独立线程，避免阻塞主线程<br>- macOS 需请求麦克风权限（`AVCaptureDevice.requestAccess`）<br>- 功耗控制：VAD 过滤后唤醒词检测 CPU 占用 < 3% |
| 预估工时 | 5 人天 |
| 依赖关系 | S1-T01 |
| 验收标准 | 1. 说出 "Hey Prometheus" 后 < 500ms 检测到唤醒<br>2. 误唤醒率 < 1 次/小时（正常办公环境）<br>3. 唤醒后录音 → STT → AI 回复完整流程通畅<br>4. CPU 占用（持续监听状态）< 3%<br>5. 灵敏度调节有效 |

### S6-T02：语音唤醒 UI 与平台集成

| 字段 | 内容 |
|------|------|
| 任务编号 | S6-T02 |
| 任务名称 | 语音唤醒前端集成与交互 |
| 具体实现内容 | 1. `PrometheusApp/Features/Voice/VoiceWakeView.swift`：语音交互 UI<br>&nbsp;&nbsp;- 唤醒状态指示器（脉冲动画）<br>&nbsp;&nbsp;- 录音波形可视化<br>&nbsp;&nbsp;- STT 实时转写显示<br>2. `PrometheusApp/Features/Voice/VoiceSettingsView.swift`：语音设置<br>&nbsp;&nbsp;- 唤醒词开关<br>&nbsp;&nbsp;- 灵敏度滑块<br>&nbsp;&nbsp;- 唤醒词选择/自定义<br>&nbsp;&nbsp;- STT 引擎选择（本地 Whisper / 云端）<br>3. macOS 集成：<br>&nbsp;&nbsp;- MenuBar 图标状态变化（监听中/唤醒/处理中）<br>&nbsp;&nbsp;- 麦克风权限请求处理<br>4. iOS 集成：<br>&nbsp;&nbsp;- 后台音频会话配置（`AVAudioSession`）<br>&nbsp;&nbsp;- 前台唤醒模式（App 打开时） |
| 技术要点 | - iOS 后台音频受限，唤醒功能仅在前台可用<br>- macOS 可后台持续监听（守护进程模式）<br>- 波形可视化使用 `AudioVisualizer`（自定义 SwiftUI Shape） |
| 预估工时 | 3 人天 |
| 依赖关系 | S6-T01, S2-T01, S3-T01 |
| 验收标准 | 1. macOS MenuBar 正确显示语音状态<br>2. 唤醒后波形动画流畅<br>3. STT 转写结果实时显示<br>4. 设置修改即时生效 |

### S6-T03：交互方式设置

| 字段 | 内容 |
|------|------|
| 任务编号 | S6-T03 |
| 任务名称 | 多模态交互方式配置系统 |
| 具体实现内容 | 1. `crates/prometheus-core/src/interaction.rs`：交互模式管理<br>&nbsp;&nbsp;- `InteractionMode` 枚举：<br>&nbsp;&nbsp;&nbsp;&nbsp;- TextChat：纯文本对话（默认）<br>&nbsp;&nbsp;&nbsp;&nbsp;- VoiceChat：语音对话（唤醒 + STT + TTS）<br>&nbsp;&nbsp;&nbsp;&nbsp;- QuickAction：快捷操作面板<br>&nbsp;&nbsp;&nbsp;&nbsp;- Background：后台静默模式（仅工作流和通知）<br>&nbsp;&nbsp;- `InteractionConfig`：每种模式的详细配置<br>&nbsp;&nbsp;- 模式切换逻辑：手动切换 / 场景自动切换<br>2. `crates/prometheus-core/src/interaction_context.rs`：场景感知<br>&nbsp;&nbsp;- 时间感知：工作时间 / 休息时间 / 深夜<br>&nbsp;&nbsp;- 设备感知：桌面端 / 移动端 / 耳机连接<br>&nbsp;&nbsp;- 自动模式推荐：深夜 → 静默模式，耳机连接 → 语音模式<br>3. `PrometheusApp/Features/Settings/InteractionSettingsView.swift`：交互设置 UI<br>&nbsp;&nbsp;- 交互模式选择<br>&nbsp;&nbsp;- 各模式详细配置<br>&nbsp;&nbsp;- 场景自动切换规则配置<br>&nbsp;&nbsp;- TTS 语音选择（系统语音列表）<br>4. TTS 集成：<br>&nbsp;&nbsp;- macOS/iOS：`AVSpeechSynthesizer`（系统 TTS）<br>&nbsp;&nbsp;- 语速/音调可调 |
| 技术要点 | - 场景感知使用简单规则引擎，不依赖 ML<br>- TTS 使用系统原生能力，零额外依赖<br>- 交互模式切换需平滑过渡（不中断当前对话） |
| 预估工时 | 3 人天 |
| 依赖关系 | S6-T01, S3-T06 |
| 验收标准 | 1. 四种交互模式可切换<br>2. 场景自动切换规则生效<br>3. TTS 朗读 AI 回复流畅<br>4. 设置持久化并同步到其他设备 |

### S6-T04：智能扫描入库

| 字段 | 内容 |
|------|------|
| 任务编号 | S6-T04 |
| 任务名称 | 文档智能扫描与知识库入库 |
| 具体实现内容 | 1. `crates/prometheus-core/src/scanner/engine.rs`：扫描引擎<br>&nbsp;&nbsp;- 支持文件类型：PDF、图片（OCR）、Markdown、TXT、DOCX、网页<br>&nbsp;&nbsp;- 文本提取：<br>&nbsp;&nbsp;&nbsp;&nbsp;- PDF：使用 `pdf-extract` crate<br>&nbsp;&nbsp;&nbsp;&nbsp;- 图片 OCR：使用 Apple Vision framework（macOS/iOS）<br>&nbsp;&nbsp;&nbsp;&nbsp;- DOCX：使用 `docx-rs` crate<br>&nbsp;&nbsp;&nbsp;&nbsp;- 网页：使用 `readability` 算法提取正文<br>2. `crates/prometheus-core/src/scanner/chunker.rs`：文本分块<br>&nbsp;&nbsp;- 智能分块策略：按段落/标题/语义边界切分<br>&nbsp;&nbsp;- 块大小：256~512 tokens（可配置）<br>&nbsp;&nbsp;- 块重叠：50 tokens（保证上下文连贯）<br>3. `crates/prometheus-core/src/scanner/embedder.rs`：向量化<br>&nbsp;&nbsp;- 本地 embedding 模型（all-MiniLM-L6-v2 ONNX）<br>&nbsp;&nbsp;- 批量向量化（batch_size=32）<br>&nbsp;&nbsp;- 向量存储到 sqlite-vec<br>4. `crates/prometheus-core/src/scanner/knowledge_base.rs`：知识库<br>&nbsp;&nbsp;- `KnowledgeBase`：文档管理 + 语义检索<br>&nbsp;&nbsp;- 检索接口：`search(query, top_k) -> Vec<Chunk>`<br>&nbsp;&nbsp;- RAG 集成：检索结果注入到对话上下文<br>5. `PrometheusApp/Features/Scanner/ScannerView.swift`：扫描 UI<br>&nbsp;&nbsp;- 文件拖拽导入<br>&nbsp;&nbsp;- 扫描进度显示<br>&nbsp;&nbsp;- 知识库浏览与搜索<br>&nbsp;&nbsp;- iOS 相机扫描（文档拍照 → OCR） |
| 技术要点 | - Embedding 模型使用 ONNX Runtime（`ort` crate）运行<br>- 模型文件约 80MB，首次使用时下载<br>- OCR 在 Apple 平台使用 VNRecognizeTextRequest（零额外依赖）<br>- RAG 检索 top_k=5，拼接到 system prompt 末尾 |
| 预估工时 | 5 人天 |
| 依赖关系 | S2-T04（sqlite-vec）, S1-T05 |
| 验收标准 | 1. PDF/图片/Markdown/DOCX 文件成功导入<br>2. 语义搜索返回相关结果（人工评估 top-5 准确率 > 70%）<br>3. RAG 对话：导入文档后，AI 可基于文档内容回答问题<br>4. iOS 相机扫描 → OCR → 入库完整流程通畅<br>5. 1000 个文档块检索延迟 < 50ms |

### S6-T05：全面联调与性能优化

| 字段 | 内容 |
|------|------|
| 任务编号 | S6-T05 |
| 任务名称 | Phase 1 全功能联调与性能优化 |
| 具体实现内容 | 1. 端到端测试场景：<br>&nbsp;&nbsp;- 场景 1：macOS 本地推理对话 → 同步到 iOS → Telegram 通知<br>&nbsp;&nbsp;- 场景 2：iPhone 语音唤醒 → 云端推理 → TTS 朗读<br>&nbsp;&nbsp;- 场景 3：扫描文档 → 知识库检索 → RAG 对话<br>&nbsp;&nbsp;- 场景 4：自动化工作流定时执行 → 多渠道通知<br>&nbsp;&nbsp;- 场景 5：安全金库存储 → 跨设备同步 → 权限控制<br>2. 性能优化：<br>&nbsp;&nbsp;- 启动时间优化：懒加载非关键模块，目标 < 2s<br>&nbsp;&nbsp;- 内存优化：模型按需加载/卸载，空闲时释放<br>&nbsp;&nbsp;- 电池优化：后台任务合并，减少唤醒频率<br>&nbsp;&nbsp;- 网络优化：请求合并、压缩、缓存<br>3. 稳定性加固：<br>&nbsp;&nbsp;- Crash 收集（macOS: `NSSetUncaughtExceptionHandler`）<br>&nbsp;&nbsp;- Rust panic hook → 日志记录而非崩溃<br>&nbsp;&nbsp;- 内存泄漏检测（Instruments Leaks）<br>&nbsp;&nbsp;- 长时间运行测试（24 小时 soak test） |
| 技术要点 | - 启动优化：使用 `lazy_static` / `once_cell` 延迟初始化<br>- Rust panic 使用 `catch_unwind` 在 FFI 边界捕获<br>- 电池优化：使用 `IOPMAssertionCreateWithName` 管理电源断言 |
| 预估工时 | 4 人天 |
| 依赖关系 | 所有前置任务 |
| 验收标准 | 1. 5 个端到端场景全部通过<br>2. macOS 冷启动 < 2s<br>3. 空闲内存占用 < 100MB（不含模型）<br>4. 24 小时 soak test 无崩溃、无内存泄漏<br>5. iOS 后台 1 小时电池消耗 < 2% |

### S6-T06：发布准备与文档

| 字段 | 内容 |
|------|------|
| 任务编号 | S6-T06 |
| 任务名称 | Phase 1 发布准备 |
| 具体实现内容 | 1. 构建与打包：<br>&nbsp;&nbsp;- macOS：签名 + 公证（notarization）+ DMG 打包<br>&nbsp;&nbsp;- iOS：TestFlight 构建上传<br>&nbsp;&nbsp;- 守护进程：Homebrew formula 或安装脚本<br>2. 版本管理：<br>&nbsp;&nbsp;- 语义化版本：v0.1.0<br>&nbsp;&nbsp;- CHANGELOG 生成<br>&nbsp;&nbsp;- Git tag 打标<br>3. 基础文档：<br>&nbsp;&nbsp;- 安装指南<br>&nbsp;&nbsp;- 快速上手指南<br>&nbsp;&nbsp;- API Key 配置说明<br>&nbsp;&nbsp;- 已知问题列表 |
| 技术要点 | - macOS 公证使用 `xcrun notarytool`<br>- DMG 使用 `create-dmg` 工具生成<br>- TestFlight 通过 `xcodebuild` + `altool` 上传 |
| 预估工时 | 3 人天 |
| 依赖关系 | S6-T05 |
| 验收标准 | 1. macOS DMG 安装后可正常运行<br>2. iOS TestFlight 安装后可正常运行<br>3. 公证通过（Gatekeeper 不拦截）<br>4. 文档完整可读 |

---

**Sprint 6 里程碑交付物：**
- [x] 语音唤醒引擎可用（唤醒 + STT + TTS）
- [x] 多模态交互方式可配置
- [x] 智能扫描入库 + RAG 检索可用
- [x] Phase 1 全功能联调通过
- [x] macOS DMG + iOS TestFlight 发布包

**Sprint 6 工时汇总：23 人天**

---

## 八、风险评估与应对方案

### 8.1 技术风险

| 风险编号 | 风险描述 | 概率 | 影响 | 应对方案 |
|----------|----------|------|------|----------|
| R01 | llama.cpp Rust 绑定不稳定，编译问题频发 | 中 | 高 | 备选方案：使用 llama.cpp HTTP server 模式，通过 HTTP API 调用；或使用 `candle` 纯 Rust 推理框架 |
| R02 | UniFFI 不支持某些复杂类型（如 Stream） | 中 | 中 | 流式响应改用 callback interface；复杂类型拆解为简单类型传递 |
| R03 | sqlite-vec 在 iOS 上加载扩展受限 | 中 | 中 | 备选方案：使用纯 Rust 向量检索库（`usearch` 或 `hnsw`），不依赖 SQLite 扩展 |
| R04 | macOS 公证流程变更或审核延迟 | 低 | 中 | 提前申请开发者账号，Sprint 4 开始试跑公证流程 |
| R05 | 本地推理在 iOS 上内存不足导致 OOM | 高 | 中 | iOS 默认路由到云端；本地推理仅支持 ≤1B 参数模型；实现内存压力监听自动卸载模型 |

### 8.2 进度风险

| 风险编号 | 风险描述 | 概率 | 影响 | 应对方案 |
|----------|----------|------|------|----------|
| R06 | 核心引擎（S1）延期，阻塞所有后续 Sprint | 中 | 极高 | S1 预留 2 天 buffer；核心引擎分配最资深工程师；必要时砍掉 S1-T06（CLI 工具）优先保障核心 |
| R07 | 多设备同步复杂度超预期 | 高 | 高 | v1 简化为"最终一致性"模型，放弃实时冲突解决；复杂冲突场景推迟到 Phase 2 |
| R08 | 消息渠道 API 变更或审核延迟 | 中 | 中 | 优先实现 Telegram（API 最开放）；WeChat 降级为 Phase 2；Slack 使用 Socket Mode 避免公网依赖 |
| R09 | 团队成员请假或离职 | 低 | 高 | 关键模块至少 2 人熟悉（代码评审制度）；核心引擎文档完善 |

### 8.3 产品风险

| 风险编号 | 风险描述 | 概率 | 影响 | 应对方案 |
|----------|----------|------|------|----------|
| R10 | 危机感知误报率过高，影响用户体验 | 中 | 中 | v1 仅启用高置信度规则；误报反馈机制；用户可关闭（但 Critical 级别不可关闭） |
| R11 | 本地推理质量不满足用户预期 | 高 | 中 | 默认路由策略设为"质量优先"（优先云端）；本地推理作为离线 fallback；随模型迭代持续改善 |
| R12 | 隐私合规风险（GDPR/个保法） | 中 | 高 | 端到端加密设计；服务端零知识架构；数据导出/删除功能；隐私政策文档 |

---

## 九、关键路径总览

```
Week 1-2 (Sprint 1):
  S1-T01 ──→ S1-T02 ──→ S1-T03 ──→ S1-T05 ──→ S1-T07
                │                       │          │
                └──→ S1-T04             └→ S1-T06  │
                                                    ↓
Week 3-4 (Sprint 2):                          S2-T01 ──→ S2-T06
  S1-T02 ──→ S2-T02                              │
  S1-T02 ──→ S2-T03                              │
  S1-T01 ──→ S2-T04 ──→ S2-T05 ──────────────────┘

Week 5-6 (Sprint 3):
  S2-T01 ──→ S3-T01 ──→ S3-T02
  S2-T04 ──→ S3-T03 ──→ S3-T04
  S2-T04 ──→ S3-T05
  S1-T05 ──→ S3-T06

Week 7-8 (Sprint 4):
  S1-T05 ──→ S4-T01 ──→ S4-T02
                │──→ S4-T03
                │──→ S4-T04
  S1-T05 ──→ S4-T05 ──→ S4-T06
  S1-T05 ──→ S4-T07

Week 9-10 (Sprint 5):
  S1-T01 ──→ S5-T01 ──→ S5-T02 ──→ S5-T03
  S5-T01 ──→ S5-T04
  S1-T05 ──→ S5-T05
  ALL ────→ S5-T06

Week 11-12 (Sprint 6):
  S1-T01 ──→ S6-T01 ──→ S6-T02
  S6-T01 ──→ S6-T03
  S2-T04 ──→ S6-T04
  ALL ────→ S6-T05 ──→ S6-T06
```

**最长关键路径：**
`S1-T01 → S1-T02 → S1-T03 → S1-T05 → S1-T07 → S2-T01 → S2-T06 → S3-T01 → S3-T04 → S6-T05 → S6-T06`

此路径上的任何延期将直接导致 Phase 1 交付延期。

---

## 十、工时汇总与资源分配

### 10.1 Sprint 工时汇总

| Sprint | 任务数 | 总工时（人天） | 日均需求（人） |
|--------|--------|----------------|----------------|
| S1 | 7 | 23 | 2.3 |
| S2 | 7 | 24 | 2.4 |
| S3 | 6 | 26 | 2.6 |
| S4 | 7 | 24 | 2.4 |
| S5 | 6 | 23 | 2.3 |
| S6 | 6 | 23 | 2.3 |
| **合计** | **39** | **143** | **平均 2.4** |

> 按 6 人团队、每人每 Sprint 可用 8 人天（扣除会议/评审/休假）计算，每 Sprint 可用 48 人天，资源充裕度 = 48/26 ≈ 1.85x，留有合理 buffer。

### 10.2 按角色分配

| 角色 | S1 | S2 | S3 | S4 | S5 | S6 | 合计 |
|------|----|----|----|----|----|----|------|
| Rust 核心工程师 ×2 | 18 | 10 | 10 | 8 | 16 | 10 | 72 |
| iOS/macOS 工程师 ×2 | 3 | 9 | 12 | 3 | 3 | 9 | 39 |
| 后端工程师 ×1 | 0 | 3 | 6 | 3 | 2 | 2 | 16 |
| 全栈工程师 ×1 | 2 | 2 | 0 | 10 | 2 | 2 | 18 |

### 10.3 Phase 1 完成标准

Phase 1 视为完成需满足以下全部条件：

1. **功能完整性**：20 个功能模块全部实现并通过验收标准
2. **质量指标**：
   - 单元测试覆盖率 > 70%（Rust 核心模块 > 80%）
   - 零 P0 bug（崩溃/数据丢失/安全漏洞）
   - P1 bug < 5 个（功能异常但有 workaround）
3. **性能指标**：
   - macOS 冷启动 < 2s
   - 本地推理首 token 延迟 < 500ms
   - 云端 API 首 token 延迟 < 1s
   - 同步延迟 < 3s
   - 空闲内存 < 100MB
4. **安全指标**：
   - 端到端加密验证通过
   - 安全金库渗透测试通过
   - API Key 不以明文存储
5. **交付物**：
   - macOS DMG 安装包（已公证）
   - iOS TestFlight 构建
   - 守护进程安装脚本
   - 用户文档（安装 + 快速上手）
