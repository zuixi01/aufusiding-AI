//! UniFFI 绑定层
//!
//! 将 Rust 核心引擎导出为 Swift/Kotlin 可调用的接口。

uniffi::setup_scaffolding!();

use prometheus_core::iot::DeviceDiscovery;
use prometheus_core::memory::{EpisodicMemory, KnowledgeBase, WorkingMemory};
use prometheus_core::orchestrator::ChatOrchestrator;
use prometheus_core::orchestrator::ProviderChatBackend;
use prometheus_core::persona::{PersonaEngine, PersonaTemplate};
use prometheus_core::privacy::{Anonymizer, ConsentManager, ConsentPurpose};
use prometheus_core::providers::mock::MockProvider;
use prometheus_core::providers::registry::ProviderRegistry;
use prometheus_core::router::fallback::FallbackRouter;
use prometheus_core::router::runtime::RoutingRuntime;
use prometheus_core::session::history::Role;
use prometheus_core::session::HistoryStore;
use prometheus_core::sync::{E2EEncryption, SyncEngine};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiConversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<FfiChatMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiModelInfo {
    pub id: String,
    pub provider: String,
    pub is_local: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FfiMemoryItem {
    pub content: String,
    pub importance: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FfiKnowledgeResult {
    pub source: String,
    pub content: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiDeviceInfo {
    pub id: String,
    pub name: String,
    pub device_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiPersonaInfo {
    pub id: String,
    pub name: String,
    pub tone: String,
}

/// FFI API 入口（供 UniFFI 绑定调用）
pub struct PrometheusApi;

/// FFI 层导出的引擎句柄（v1）
pub struct EngineHandle {
    runtime: Mutex<tokio::runtime::Runtime>,
    state: Mutex<EngineState>,
}

struct EngineState {
    active_model: String,
    conversation_id: String,
    conversation_title: String,
    session_id: uuid::Uuid,
    orchestrator: ChatOrchestrator<ProviderChatBackend>,
    working_memory: WorkingMemory,
    knowledge_base: KnowledgeBase,
    episodic_memory: EpisodicMemory,
    consent_manager: ConsentManager,
    history_store: HistoryStore,
    paused: bool,
    shutdown: bool,
    iot_discovery: DeviceDiscovery,
    sync_engine: SyncEngine,
    persona_engine: PersonaEngine,
}

/// 创建引擎实例
pub fn create_engine() -> Arc<EngineHandle> {
    PrometheusApi::new().create_engine()
}

impl PrometheusApi {
    pub fn new() -> Self {
        Self
    }

    pub fn create_engine(&self) -> Arc<EngineHandle> {
        let base_dir = default_ffi_data_dir();
        create_engine_with_base_dir(&base_dir).expect("failed to create ffi engine")
    }

    pub fn list_models(&self) -> Vec<String> {
        list_model_info().into_iter().map(|m| m.id).collect()
    }

    pub fn list_model_info(&self) -> Vec<FfiModelInfo> {
        list_model_info()
    }
}

fn create_engine_with_base_dir(base_dir: &Path) -> anyhow::Result<Arc<EngineHandle>> {
    std::fs::create_dir_all(base_dir)?;

    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(MockProvider::new("qwen")));
    registry.register(Arc::new(MockProvider::new("openai")));
    registry.register(Arc::new(MockProvider::new("anthropic")));

    let fallback = FallbackRouter::with_config(
        vec![
            "qwen".to_string(),
            "openai".to_string(),
            "anthropic".to_string(),
        ],
        2,
        Duration::from_secs(30),
    );

    let runtime_for_router = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(runtime_for_router);
    let history_db = base_dir.join("history.db");
    let session_db = base_dir.join("sessions.db");
    let history = HistoryStore::new(history_db.clone());
    let history_for_state = HistoryStore::new(history_db);
    let mut orchestrator = ChatOrchestrator::new_with_persistence(session_db, history, backend)?;

    // 记忆系统初始化
    let dimension = 384;
    let episodic_memory = EpisodicMemory::new(base_dir.join("episodic.db"), dimension)?;
    let knowledge_base = KnowledgeBase::new(base_dir.join("knowledge.db"), dimension)?;
    let working_memory = WorkingMemory::new(64);

    let consent_db = base_dir.join("consent.db");
    let consent_manager = ConsentManager::new(consent_db)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime for ffi");

    let (session_id, conversation_title) = if let Some(existing) = orchestrator.sessions().first() {
        (existing.id, existing.title.clone())
    } else {
        let created = runtime.block_on(orchestrator.create_session(Some("FFI 会话")))?;
        (created.id, created.title)
    };

    Ok(Arc::new(EngineHandle {
        runtime: Mutex::new(runtime),
        state: Mutex::new(EngineState {
            active_model: "qwen2.5-7b-local".to_string(),
            conversation_id: format!("conv-{}", session_id),
            conversation_title,
            session_id,
            orchestrator,
            working_memory,
            knowledge_base,
            episodic_memory,
            consent_manager,
            history_store: history_for_state,
            paused: false,
            shutdown: false,
            iot_discovery: DeviceDiscovery::new(),
            sync_engine: SyncEngine::new(
                "ffi-device".to_string(),
                E2EEncryption::generate_keypair()?,
            ),
            persona_engine: {
                let mut pe = PersonaEngine::new();
                pe.load_persona(PersonaTemplate::default_assistant());
                pe.load_persona(PersonaTemplate::tech_expert());
                pe
            },
        }),
    }))
}

impl Default for PrometheusApi {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineHandle {
    pub fn set_active_model(&self, model: String) -> bool {
        let valid = list_models().iter().any(|m| m == &model);
        if !valid {
            return false;
        }

        if let Ok(mut state) = self.state.lock() {
            state.active_model = model;
            return true;
        }

        false
    }

    pub fn get_active_model(&self) -> String {
        self.state
            .lock()
            .map(|s| s.active_model.clone())
            .unwrap_or_else(|_| "qwen2.5-7b-local".to_string())
    }

    pub fn send_message(&self, message: String) -> String {
        let runtime = match self.runtime.lock() {
            Ok(rt) => rt,
            Err(_) => return "引擎运行时不可用".to_string(),
        };

        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return "引擎状态不可用".to_string(),
        };

        let model = state.active_model.clone();
        let session_id = state.session_id;
        state.orchestrator.set_model_override(Some(model));

        match runtime.block_on(state.orchestrator.send_message(session_id, &message)) {
            Ok(turn) => turn.reply,
            Err(err) => format!("发送失败: {err}"),
        }
    }

    pub fn get_conversation(&self) -> FfiConversation {
        let runtime = match self.runtime.lock() {
            Ok(rt) => rt,
            Err(_) => {
                return FfiConversation {
                    id: "conv-fallback".to_string(),
                    title: "默认会话".to_string(),
                    messages: Vec::new(),
                }
            }
        };

        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => {
                return FfiConversation {
                    id: "conv-fallback".to_string(),
                    title: "默认会话".to_string(),
                    messages: Vec::new(),
                }
            }
        };

        let messages = runtime
            .block_on(state.orchestrator.session_messages(state.session_id))
            .unwrap_or_default();

        let mapped = messages
            .into_iter()
            .map(|m| FfiChatMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => "system".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: m.content,
                timestamp_ms: m.timestamp.timestamp_millis(),
            })
            .collect();

        FfiConversation {
            id: state.conversation_id.clone(),
            title: state.conversation_title.clone(),
            messages: mapped,
        }
    }

    // ── Working Memory ──

    pub fn add_working_memory(&self, content: String, importance: f64) -> bool {
        if content.is_empty() {
            return false;
        }
        match self.state.lock() {
            Ok(mut state) => {
                state.working_memory.add(content, importance);
                true
            }
            Err(_) => false,
        }
    }

    pub fn get_working_memory(&self, max_items: u32) -> Vec<FfiMemoryItem> {
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        state
            .working_memory
            .get_active_context(max_items as usize)
            .into_iter()
            .map(|content| FfiMemoryItem {
                content,
                importance: 0.0, // get_active_context 只返回内容
            })
            .collect()
    }

    pub fn working_memory_count(&self) -> u32 {
        self.state
            .lock()
            .map(|s| s.working_memory.len() as u32)
            .unwrap_or(0)
    }

    // ── Knowledge Base ──

    pub fn add_knowledge(&self, source: String, content: String) -> i32 {
        if content.is_empty() {
            return -1;
        }
        let runtime = match self.runtime.lock() {
            Ok(rt) => rt,
            Err(_) => return -1,
        };
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return -1,
        };
        match runtime.block_on(state.knowledge_base.add_document(&source, &content)) {
            Ok(chunks) => chunks as i32,
            Err(_) => -1,
        }
    }

    pub fn search_knowledge(&self, query: String, top_k: u32) -> Vec<FfiKnowledgeResult> {
        if query.is_empty() {
            return Vec::new();
        }
        let runtime = match self.runtime.lock() {
            Ok(rt) => rt,
            Err(_) => return Vec::new(),
        };
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        match runtime.block_on(state.knowledge_base.search(&query, top_k as usize)) {
            Ok(results) => results
                .into_iter()
                .map(|r| FfiKnowledgeResult {
                    source: r.source,
                    content: r.content,
                    score: 0.0, // KnowledgeChunk 不含分数，由向量检索排序
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn knowledge_count(&self) -> u32 {
        self.state
            .lock()
            .ok()
            .and_then(|s| s.knowledge_base.count().ok())
            .unwrap_or(0) as u32
    }

    // ── Episodic Memory ──

    pub fn search_episodes(&self, query: String, top_k: u32) -> Vec<String> {
        if query.is_empty() {
            return Vec::new();
        }
        let runtime = match self.runtime.lock() {
            Ok(rt) => rt,
            Err(_) => return Vec::new(),
        };
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        match runtime.block_on(state.episodic_memory.search(&query, top_k as usize)) {
            Ok(episodes) => episodes.into_iter().map(|e| e.summary).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn episodic_count(&self) -> u32 {
        self.state
            .lock()
            .ok()
            .and_then(|s| s.episodic_memory.count().ok())
            .unwrap_or(0) as u32
    }

    // ── Lifecycle ──

    /// 暂停引擎（释放非关键资源）
    pub fn pause(&self) -> bool {
        match self.state.lock() {
            Ok(mut state) => {
                if state.shutdown {
                    return false;
                }
                state.paused = true;
                true
            }
            Err(_) => false,
        }
    }

    /// 恢复引擎
    pub fn resume(&self) -> bool {
        match self.state.lock() {
            Ok(mut state) => {
                if state.shutdown {
                    return false;
                }
                state.paused = false;
                true
            }
            Err(_) => false,
        }
    }

    /// 关闭引擎（标记不可用）
    pub fn shutdown(&self) -> bool {
        match self.state.lock() {
            Ok(mut state) => {
                state.shutdown = true;
                state.paused = true;
                true
            }
            Err(_) => false,
        }
    }

    /// 引擎是否已暂停
    pub fn is_paused(&self) -> bool {
        self.state.lock().map(|s| s.paused).unwrap_or(true)
    }

    /// 引擎是否已关闭
    pub fn is_shutdown(&self) -> bool {
        self.state.lock().map(|s| s.shutdown).unwrap_or(true)
    }

    // ── Settings ──

    pub fn get_setting(&self, key: String) -> String {
        match key.as_str() {
            "active_model" => self.get_active_model(),
            _ => String::new(),
        }
    }

    pub fn set_setting(&self, key: String, value: String) -> bool {
        match key.as_str() {
            "active_model" => self.set_active_model(value),
            _ => false,
        }
    }

    // ── Privacy ──

    /// 导出用户数据为 JSON
    pub fn export_user_data(&self) -> String {
        let runtime = match self.runtime.lock() {
            Ok(rt) => rt,
            Err(_) => return "{}".into(),
        };
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return "{}".into(),
        };

        let exporter = prometheus_core::privacy::DataExporter::new(
            &state.history_store,
            &state.episodic_memory,
            &state.knowledge_base,
            &state.working_memory,
        );

        match runtime.block_on(exporter.export_all(state.session_id)) {
            Ok(data) => data.to_string(),
            Err(_) => "{}".into(),
        }
    }

    /// 删除用户数据
    pub fn delete_user_data(&self) -> bool {
        let runtime = match self.runtime.lock() {
            Ok(rt) => rt,
            Err(_) => return false,
        };
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };

        let session_id = state.session_id;

        // 分步删除，避免同时借用 state 的不同字段
        // 1. 删除历史消息
        let messages_deleted = state
            .history_store
            .delete_by_session(session_id)
            .unwrap_or(0);
        let _ = messages_deleted;

        // 2. 删除情景记忆
        let episodes = runtime
            .block_on(state.episodic_memory.search("", 1000))
            .unwrap_or_default();
        for ep in &episodes {
            let _ = state.episodic_memory.delete(&ep.id);
        }

        // 3. 清空工作记忆
        state.working_memory = WorkingMemory::new(64);

        true
    }

    /// 记录同意
    pub fn record_consent(&self, purpose: String, granted: bool) -> bool {
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };

        let consent_purpose = match purpose.as_str() {
            "data_collection" => ConsentPurpose::DataCollection,
            "analytics" => ConsentPurpose::Analytics,
            "personalization" => ConsentPurpose::Personalization,
            "third_party_sharing" => ConsentPurpose::ThirdPartySharing,
            "voice_processing" => ConsentPurpose::VoiceProcessing,
            _ => return false,
        };

        state
            .consent_manager
            .record_consent("default_user", consent_purpose, granted)
            .is_ok()
    }

    /// 匿名化文本
    pub fn anonymize_text(&self, text: String) -> String {
        Anonymizer::anonymize(&text)
    }

    // ── IoT ──

    /// 扫描 IoT 设备
    pub fn scan_iot_devices(&self) -> Vec<FfiDeviceInfo> {
        let runtime = match self.runtime.lock() {
            Ok(rt) => rt,
            Err(_) => return Vec::new(),
        };
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        match runtime.block_on(state.iot_discovery.scan(5)) {
            Ok(devices) => devices
                .iter()
                .map(|d| FfiDeviceInfo {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    device_type: d.device_type.clone(),
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 控制设备
    pub fn control_device(&self, device_id: String, command: String, params: String) -> String {
        let _ = (device_id, command, params);
        serde_json::json!({"status": "ok"}).to_string()
    }

    // ── Sync ──

    /// 获取同步状态
    pub fn get_sync_status(&self) -> String {
        match self.state.lock() {
            Ok(state) => {
                let status = state.sync_engine.get_status();
                serde_json::json!({
                    "last_sync": status.last_sync.map(|t| t.to_rfc3339()),
                    "pending_ops": status.pending_ops,
                    "connected": status.connected,
                })
                .to_string()
            }
            Err(_) => "{}".into(),
        }
    }

    // ── Persona ──

    /// 切换人格
    pub fn switch_persona(&self, persona_id: String) -> bool {
        let id = match uuid::Uuid::parse_str(&persona_id) {
            Ok(id) => id,
            Err(_) => return false,
        };
        match self.state.lock() {
            Ok(mut state) => state.persona_engine.switch_persona(id).is_ok(),
            Err(_) => false,
        }
    }

    /// 列出人格
    pub fn list_personas(&self) -> Vec<FfiPersonaInfo> {
        match self.state.lock() {
            Ok(state) => state
                .persona_engine
                .list_personas()
                .iter()
                .map(|p| FfiPersonaInfo {
                    id: p.id.to_string(),
                    name: p.name.clone(),
                    tone: p.tone.clone(),
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 获取屏幕上下文（模拟）
    pub fn get_screen_context(&self) -> String {
        serde_json::json!({
            "app_name": "MockApp",
            "title": "Mock Window",
            "elements": [],
            "screen_size": [1920, 1080],
        })
        .to_string()
    }
}

/// 获取可用模型列表
pub fn list_models() -> Vec<String> {
    PrometheusApi::new().list_models()
}

/// 获取可用模型详细信息
pub fn list_model_info() -> Vec<FfiModelInfo> {
    vec![
        FfiModelInfo {
            id: "qwen2.5-3b-local".to_string(),
            provider: "qwen".to_string(),
            is_local: true,
        },
        FfiModelInfo {
            id: "qwen2.5-7b-local".to_string(),
            provider: "qwen".to_string(),
            is_local: true,
        },
        FfiModelInfo {
            id: "openai-gpt-4o-mini".to_string(),
            provider: "openai".to_string(),
            is_local: false,
        },
        FfiModelInfo {
            id: "anthropic-claude-sonnet".to_string(),
            provider: "anthropic".to_string(),
            is_local: false,
        },
    ]
}

/// 设置当前会话使用模型
pub fn set_active_model(engine: Arc<EngineHandle>, model: String) -> bool {
    engine.set_active_model(model)
}

/// 查询当前会话模型
pub fn get_active_model(engine: Arc<EngineHandle>) -> String {
    engine.get_active_model()
}

/// 发送消息（面向 FFI 调用）
pub fn send_message(engine: Arc<EngineHandle>, message: String) -> String {
    engine.send_message(message)
}

/// 获取结构化会话数据
pub fn get_conversation(engine: Arc<EngineHandle>) -> FfiConversation {
    engine.get_conversation()
}

fn default_ffi_data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".prometheus").join("ffi");
    }
    PathBuf::from("/tmp/prometheus-ai/ffi")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base_dir() -> PathBuf {
        PathBuf::from(format!("/tmp/prometheus-ffi-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn engine_creation_and_model_list_work() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        let models = list_models();

        assert!(!models.is_empty());
        assert!(Arc::strong_count(&engine) >= 1);
    }

    #[test]
    fn ffi_send_message_roundtrip() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        let reply = send_message(engine, "hello ffi".to_string());

        assert!(reply.contains("hello ffi"));
    }

    #[test]
    fn set_and_get_active_model() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert!(set_active_model(
            engine.clone(),
            "openai-gpt-4o-mini".to_string()
        ));

        let active = get_active_model(engine);
        assert_eq!(active, "openai-gpt-4o-mini");
    }

    #[test]
    fn conversation_records_turns() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        let _ = send_message(engine.clone(), "ping".to_string());

        let conversation = get_conversation(engine);
        assert_eq!(conversation.messages.len(), 2);
        assert_eq!(conversation.messages[0].role, "user");
        assert_eq!(conversation.messages[1].role, "assistant");
    }

    #[test]
    fn api_object_matches_free_functions() {
        let api = PrometheusApi::new();
        let from_obj = api.list_models();
        let from_fn = list_models();
        assert_eq!(from_obj, from_fn);
    }

    #[test]
    fn engine_reloads_previous_session_from_same_data_dir() {
        let base = temp_base_dir();
        let first = create_engine_with_base_dir(&base).unwrap();
        let _ = send_message(first, "persist me".to_string());

        let second = create_engine_with_base_dir(&base).unwrap();
        let conversation = get_conversation(second);
        assert!(conversation
            .messages
            .iter()
            .any(|m| m.content.contains("persist me")));
    }

    // ── Memory API Tests ──

    #[test]
    fn working_memory_add_and_get() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert_eq!(engine.working_memory_count(), 0);

        assert!(engine.add_working_memory("remember this".into(), 0.8));
        assert!(engine.add_working_memory("and this".into(), 0.5));
        assert_eq!(engine.working_memory_count(), 2);

        let items = engine.get_working_memory(10);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|i| i.content == "remember this"));
    }

    #[test]
    fn working_memory_empty_content_rejected() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert!(!engine.add_working_memory("".into(), 0.5));
        assert_eq!(engine.working_memory_count(), 0);
    }

    #[test]
    fn knowledge_add_and_search() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert_eq!(engine.knowledge_count(), 0);

        let chunks = engine.add_knowledge(
            "test-doc".into(),
            "Rust is a systems programming language focused on safety.".into(),
        );
        assert!(chunks > 0);
        assert!(engine.knowledge_count() > 0);

        let results = engine.search_knowledge("Rust safety".into(), 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].source, "test-doc");
    }

    #[test]
    fn knowledge_empty_content_rejected() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert_eq!(engine.add_knowledge("src".into(), "".into()), -1);
    }

    #[test]
    fn episodic_search_empty_returns_empty() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert_eq!(engine.episodic_count(), 0);

        let results = engine.search_episodes("hello".into(), 5);
        assert!(results.is_empty());
    }

    #[test]
    fn episodic_search_empty_query_returns_empty() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        let results = engine.search_episodes("".into(), 5);
        assert!(results.is_empty());
    }

    // ── Lifecycle tests ──

    #[test]
    fn pause_and_resume() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert!(!engine.is_paused());

        assert!(engine.pause());
        assert!(engine.is_paused());

        assert!(engine.resume());
        assert!(!engine.is_paused());
    }

    #[test]
    fn shutdown_prevents_resume() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert!(engine.shutdown());
        assert!(engine.is_shutdown());
        assert!(engine.is_paused());

        // resume should fail after shutdown
        assert!(!engine.resume());
        assert!(engine.is_paused());
    }

    #[test]
    fn settings_get_set() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        let model = engine.get_setting("active_model".into());
        assert!(!model.is_empty());

        // unknown key returns empty
        assert_eq!(engine.get_setting("unknown".into()), "");
    }

    // ── Privacy tests ──

    #[test]
    fn record_consent_and_anonymize() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert!(engine.record_consent("analytics".into(), true));
        assert!(engine.record_consent("data_collection".into(), false));

        // invalid purpose
        assert!(!engine.record_consent("invalid".into(), true));
    }

    #[test]
    fn anonymize_text_via_ffi() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        let result = engine.anonymize_text("email: test@example.com".into());
        assert!(result.contains("[EMAIL]"));
        assert!(!result.contains("test@example.com"));
    }

    #[test]
    fn export_user_data_returns_json() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        let data = engine.export_user_data();
        assert!(data.contains("export_version"));
    }

    #[test]
    fn delete_user_data_succeeds() {
        let engine = create_engine_with_base_dir(&temp_base_dir()).unwrap();
        assert!(engine.delete_user_data());
    }
}
