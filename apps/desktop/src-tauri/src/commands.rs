use serde_json::Value;
use std::sync::Mutex;
use tauri::State;

use prometheus_core::iot::DeviceDiscovery;
use prometheus_core::memory::{EpisodicMemory, KnowledgeBase, WorkingMemory};
use prometheus_core::orchestrator::{ChatOrchestrator, ProviderChatBackend};
use prometheus_core::persona::{PersonaEngine, PersonaTemplate};
use prometheus_core::privacy::{ConsentManager, DataExporter};
use prometheus_core::providers::mock::MockProvider;
use prometheus_core::providers::ollama::OllamaProvider;
use prometheus_core::providers::registry::ProviderRegistry;
use prometheus_core::router::fallback::FallbackRouter;
use prometheus_core::router::runtime::RoutingRuntime;
use prometheus_core::session::history::Role;
use prometheus_core::session::HistoryStore;
use prometheus_core::sync::{E2EEncryption, SyncEngine};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct AppState {
    pub runtime: Mutex<tokio::runtime::Runtime>,
    pub orchestrator: Mutex<ChatOrchestrator<ProviderChatBackend>>,
    pub session_id: uuid::Uuid,
    pub active_model: Mutex<String>,
    pub working_memory: Mutex<WorkingMemory>,
    pub knowledge_base: Mutex<KnowledgeBase>,
    pub episodic_memory: Mutex<EpisodicMemory>,
    #[allow(dead_code)]
    pub consent_manager: Mutex<ConsentManager>,
    pub history_store: Mutex<HistoryStore>,
    pub iot_discovery: Mutex<DeviceDiscovery>,
    pub sync_engine: Mutex<SyncEngine>,
    pub persona_engine: Mutex<PersonaEngine>,
}

pub fn create_app_state() -> anyhow::Result<AppState> {
    let base_dir = default_data_dir();
    std::fs::create_dir_all(&base_dir)?;

    let mut registry = ProviderRegistry::new();

    // 优先检测 Ollama 可用性
    let ollama_available = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()
        .and_then(|c| c.get("http://localhost:11434/api/tags").send().ok())
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    let provider_names = if ollama_available {
        registry.register(Arc::new(OllamaProvider::new()));
        vec!["ollama".to_string()]
    } else {
        registry.register(Arc::new(MockProvider::new("qwen")));
        registry.register(Arc::new(MockProvider::new("openai")));
        registry.register(Arc::new(MockProvider::new("anthropic")));
        vec!["qwen".into(), "openai".into(), "anthropic".into()]
    };

    let fallback = FallbackRouter::with_config(provider_names, 2, Duration::from_secs(30));

    let routing = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(routing);
    let history_db = base_dir.join("history.db");
    let session_db = base_dir.join("sessions.db");
    let history = HistoryStore::new(history_db.clone());
    let history_for_state = HistoryStore::new(history_db);

    let mut orchestrator = ChatOrchestrator::new_with_persistence(session_db, history, backend)?;

    let dimension = 384;
    let episodic_memory = EpisodicMemory::new(base_dir.join("episodic.db"), dimension)?;
    let knowledge_base = KnowledgeBase::new(base_dir.join("knowledge.db"), dimension)?;
    let working_memory = WorkingMemory::new(64);
    let consent_manager = ConsentManager::new(base_dir.join("consent.db"))?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let session_id = if let Some(existing) = orchestrator.sessions().first() {
        existing.id
    } else {
        let created = runtime.block_on(orchestrator.create_session(Some("Desktop 会话")))?;
        created.id
    };

    let mut persona_engine = PersonaEngine::new();
    persona_engine.load_persona(PersonaTemplate::default_assistant());
    persona_engine.load_persona(PersonaTemplate::tech_expert());

    Ok(AppState {
        runtime: Mutex::new(runtime),
        orchestrator: Mutex::new(orchestrator),
        session_id,
        active_model: Mutex::new("qwen2.5-7b-local".to_string()),
        working_memory: Mutex::new(working_memory),
        knowledge_base: Mutex::new(knowledge_base),
        episodic_memory: Mutex::new(episodic_memory),
        consent_manager: Mutex::new(consent_manager),
        history_store: Mutex::new(history_for_state),
        iot_discovery: Mutex::new(DeviceDiscovery::new()),
        sync_engine: Mutex::new(SyncEngine::new(
            "desktop-device".to_string(),
            E2EEncryption::generate_keypair()?,
        )),
        persona_engine: Mutex::new(persona_engine),
    })
}

pub fn default_data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".prometheus").join("desktop");
    }
    PathBuf::from("/tmp/prometheus-ai/desktop")
}

// MARK: - Tauri Commands

#[tauri::command]
pub fn send_message(message: String, state: State<AppState>) -> String {
    let rt = state.runtime.lock().unwrap();
    let mut orch = state.orchestrator.lock().unwrap();
    let model = state.active_model.lock().unwrap().clone();
    orch.set_model_override(Some(model));
    match rt.block_on(orch.send_message(state.session_id, &message)) {
        Ok(turn) => turn.reply,
        Err(e) => format!("发送失败: {e}"),
    }
}

#[tauri::command]
pub fn get_conversation(state: State<AppState>) -> Value {
    let rt = state.runtime.lock().unwrap();
    let orch = state.orchestrator.lock().unwrap();
    let messages = rt
        .block_on(orch.session_messages(state.session_id))
        .unwrap_or_default();
    let mapped: Vec<Value> = messages
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "role": match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => "system",
                    Role::Tool => "tool",
                },
                "content": m.content,
                "timestamp_ms": m.timestamp.timestamp_millis(),
            })
        })
        .collect();
    serde_json::json!({ "messages": mapped })
}

#[tauri::command]
pub fn list_models() -> Vec<Value> {
    vec![
        serde_json::json!({"id": "qwen2.5-3b-local", "provider": "qwen", "is_local": true}),
        serde_json::json!({"id": "qwen2.5-7b-local", "provider": "qwen", "is_local": true}),
        serde_json::json!({"id": "openai-gpt-4o-mini", "provider": "openai", "is_local": false}),
        serde_json::json!({"id": "anthropic-claude-sonnet", "provider": "anthropic", "is_local": false}),
    ]
}

#[tauri::command]
pub fn switch_model(model_id: String, state: State<AppState>) -> bool {
    let valid_ids = [
        "qwen2.5-3b-local",
        "qwen2.5-7b-local",
        "openai-gpt-4o-mini",
        "anthropic-claude-sonnet",
    ];
    if !valid_ids.contains(&model_id.as_str()) {
        return false;
    }
    *state.active_model.lock().unwrap() = model_id;
    true
}

#[tauri::command]
pub fn scan_devices(state: State<AppState>) -> Vec<Value> {
    let rt = state.runtime.lock().unwrap();
    let mut disc = state.iot_discovery.lock().unwrap();
    match rt.block_on(disc.scan(5)) {
        Ok(devices) => devices
            .iter()
            .map(|d| serde_json::json!({"id": d.id, "name": d.name, "device_type": d.device_type}))
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[tauri::command]
pub fn control_device(_device_id: String, _command: String, _params: String) -> String {
    serde_json::json!({"status": "ok"}).to_string()
}

#[tauri::command]
pub fn list_personas(state: State<AppState>) -> Vec<Value> {
    let pe = state.persona_engine.lock().unwrap();
    pe.list_personas()
        .iter()
        .map(|p| serde_json::json!({"id": p.id.to_string(), "name": p.name, "tone": p.tone}))
        .collect()
}

#[tauri::command]
pub fn switch_persona(persona_id: String, state: State<AppState>) -> bool {
    let id = match uuid::Uuid::parse_str(&persona_id) {
        Ok(id) => id,
        Err(_) => return false,
    };
    state
        .persona_engine
        .lock()
        .unwrap()
        .switch_persona(id)
        .is_ok()
}

#[tauri::command]
pub fn get_sync_status(state: State<AppState>) -> Value {
    let se = state.sync_engine.lock().unwrap();
    let status = se.get_status();
    serde_json::json!({
        "last_sync": status.last_sync.map(|t| t.to_rfc3339()),
        "pending_ops": status.pending_ops,
        "connected": status.connected,
    })
}

#[tauri::command]
pub fn search_knowledge(query: String, top_k: u32, state: State<AppState>) -> Vec<Value> {
    if query.is_empty() {
        return Vec::new();
    }
    let rt = state.runtime.lock().unwrap();
    let kb = state.knowledge_base.lock().unwrap();
    match rt.block_on(kb.search(&query, top_k as usize)) {
        Ok(results) => results
            .into_iter()
            .map(|r| serde_json::json!({"source": r.source, "content": r.content}))
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[tauri::command]
pub fn export_data(state: State<AppState>) -> String {
    let rt = state.runtime.lock().unwrap();
    let hs = state.history_store.lock().unwrap();
    let em = state.episodic_memory.lock().unwrap();
    let kb = state.knowledge_base.lock().unwrap();
    let wm = state.working_memory.lock().unwrap();
    let exporter = DataExporter::new(&*hs, &*em, &*kb, &*wm);
    match rt.block_on(exporter.export_all(state.session_id)) {
        Ok(data) => data.to_string(),
        Err(_) => "{}".into(),
    }
}

#[tauri::command]
pub fn delete_data(state: State<AppState>) -> bool {
    let rt = state.runtime.lock().unwrap();
    let _ = state
        .history_store
        .lock()
        .unwrap()
        .delete_by_session(state.session_id);
    let episodes = rt
        .block_on(state.episodic_memory.lock().unwrap().search("", 1000))
        .unwrap_or_default();
    for ep in &episodes {
        let _ = state.episodic_memory.lock().unwrap().delete(&ep.id);
    }
    *state.working_memory.lock().unwrap() = WorkingMemory::new(64);
    true
}
