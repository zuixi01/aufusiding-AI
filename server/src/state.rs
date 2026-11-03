//! 应用共享状态

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use prometheus_core::iot::{DeviceDiscovery, GenericController, SecurePairing};
use prometheus_core::memory::{EpisodicMemory, KnowledgeBase, WorkingMemory};
use prometheus_core::orchestrator::{ChatOrchestrator, ProviderChatBackend};
use prometheus_core::persona::PersonaEngine;
use prometheus_core::privacy::ConsentManager;
use prometheus_core::security::{InputValidator, PromptGuard, RateLimiter, SecurityAuditLog};
use prometheus_core::session::HistoryStore;
use prometheus_core::sync::SyncEngine;

use crate::audit::AuditWriter;
use crate::metrics::Metrics;

/// 服务器共享状态
#[derive(Clone)]
pub struct AppState {
    /// 聊天编排器
    pub orchestrator: Arc<RwLock<ChatOrchestrator<ProviderChatBackend>>>,
    /// 历史存储
    pub history_store: Arc<HistoryStore>,
    /// 情景记忆 (L2)
    pub episodic_memory: Arc<RwLock<EpisodicMemory>>,
    /// 知识库 (L3)
    pub knowledge_base: Arc<RwLock<KnowledgeBase>>,
    /// 工作记忆 (L1)
    pub working_memory: Arc<RwLock<WorkingMemory>>,
    /// 同意管理器
    pub consent_manager: Arc<RwLock<ConsentManager>>,
    /// 数据目录
    #[allow(dead_code)]
    pub data_dir: PathBuf,
    /// 指标收集器
    pub metrics: Metrics,
    /// IoT 设备发现
    pub iot_discovery: Arc<RwLock<DeviceDiscovery>>,
    /// IoT 安全配对
    pub iot_pairing: Arc<RwLock<SecurePairing>>,
    /// IoT 设备控制
    pub iot_controller: Arc<RwLock<GenericController>>,
    /// 同步引擎
    pub sync_engine: Arc<RwLock<SyncEngine>>,
    /// 人格引擎
    pub persona_engine: Arc<RwLock<PersonaEngine>>,
    /// Prompt 注入防护
    pub prompt_guard: Arc<PromptGuard>,
    /// 输入验证器
    pub input_validator: Arc<InputValidator>,
    /// 速率限制器
    pub rate_limiter: Arc<RateLimiter>,
    /// 安全审计日志
    pub audit_log: Arc<SecurityAuditLog>,
    /// 异步审计写入器
    pub audit_writer: AuditWriter,
    /// 认证配置
    pub auth_config: crate::auth::AuthConfig,
}
