# API 参考

## 核心引擎 API

### Router（智能路由）

```rust
// 意图分类
let classifier = IntentClassifier::new();
let result = classifier.classify("帮我写一段代码").await?;

// 路由决策
let strategy = RoutingStrategy::new();
let decision = strategy.decide(&result);
```

### Session（对话管理）

```rust
// 创建会话
let mut manager = SessionManager::new();
let session = manager.create_session(Some("技术讨论")).await?;

// 恢复会话
let session = manager.resume_session(session_id).await?;
```

### Memory（记忆系统）

```rust
// 用户画像
let profile = UserProfile::new("user_001");

// 情景记忆检索
let memory = EpisodicMemory::new(db_path);
let episodes = memory.recall(&query_embedding, 5).await?;

// 主动回忆
let recall = ProactiveRecall::new(0.7);
let results = recall.recall("当前上下文").await?;
```

### Providers（云端 API）

```rust
// OpenAI
let provider = OpenAiProvider::new("sk-...");
let response = provider.chat(&request).await?;

// Anthropic
let provider = AnthropicProvider::new("sk-ant-...");
let response = provider.chat(&request).await?;
```

## FFI 接口

通过 UniFFI 导出的跨语言接口：

```swift
// Swift
let chat = PrometheusChat()
let response = chat.sendMessage("你好")
```

## 插件 SDK

```rust
use prometheus_plugin_sdk::Plugin;

struct MyPlugin;

impl Plugin for MyPlugin {
    fn name(&self) -> &str { "my-plugin" }
    fn version(&self) -> &str { "0.1.0" }
    fn init(&mut self) -> Result<(), String> { Ok(()) }
    fn execute(&self, input: &str) -> Result<String, String> { Ok(input.to_string()) }
}
```
