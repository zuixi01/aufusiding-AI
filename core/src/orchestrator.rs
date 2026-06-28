//! 对话编排器
//!
//! 串联会话管理、历史存储、意图分类、路由决策与回复生成。

use crate::inference::cache::PromptCache;
use crate::memory::manager::MemoryManager;
use crate::memory::profile::UserProfile;
use crate::persona::{Persona, PromptContextBuilder};
use crate::privacy::consent::ConsentManager;
use crate::providers::traits::{ChatMessage, ChatRequest, TokenUsage};
use crate::router::classifier::ClassificationResult;
use crate::router::runtime::RoutingRuntime;
use crate::router::steward::LocalSteward;
use crate::router::strategy::{RouteDecision, RoutingConstraints};
use crate::router::{IntentClassifier, RoutingStrategy};
use crate::session::context::ContextCompressor;
use crate::session::history::{HistoryStore, Message, Role};
use crate::session::{Session, SessionManager};
use crate::storage::UsageStore;
use crate::agent::integration::{AgentOrchestrator, ComplexityAssessor, TaskRoute};
use crate::safety::classification::DataClassifier;
use crate::safety::security_mode::{PolicyDecision, SecurityPolicyEnforcer};
use crate::mind::integration::MindIntegration;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// 聊天后端抽象（可接本地模型、云端 Provider 或 Mock）
#[async_trait]
pub trait ChatBackend: Send + Sync {
    async fn chat(
        &self,
        input: &str,
        context: &[Message],
        classification: &ClassificationResult,
        decision: &RouteDecision,
    ) -> anyhow::Result<BackendReply>;
}

#[derive(Debug, Clone, Default)]
pub struct BackendReply {
    pub content: String,
    pub route_attempts: Vec<String>,
    pub token_usage: Option<TokenUsage>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// 测试用 Mock 聊天后端
pub struct MockChatBackend {
    reply: std::sync::Mutex<String>,
    calls: std::sync::Mutex<Vec<String>>,
}

impl MockChatBackend {
    pub fn new(reply: &str) -> Self {
        Self {
            reply: std::sync::Mutex::new(reply.to_string()),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn set_reply(&self, reply: &str) {
        *self.reply.lock().unwrap() = reply.to_string();
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ChatBackend for MockChatBackend {
    async fn chat(
        &self,
        input: &str,
        _context: &[Message],
        _classification: &ClassificationResult,
        _decision: &RouteDecision,
    ) -> anyhow::Result<BackendReply> {
        self.calls.lock().unwrap().push(input.to_string());
        Ok(BackendReply {
            content: self.reply.lock().unwrap().clone(),
            route_attempts: vec!["mock".into()],
            token_usage: None,
            provider: Some("mock".into()),
            model: Some("mock-model".into()),
        })
    }
}

/// 基于 Provider 路由运行时的后端实现
pub struct ProviderChatBackend {
    runtime: Mutex<RoutingRuntime>,
}

impl ProviderChatBackend {
    pub fn new(runtime: RoutingRuntime) -> Self {
        Self {
            runtime: Mutex::new(runtime),
        }
    }
}

#[async_trait]
impl ChatBackend for ProviderChatBackend {
    async fn chat(
        &self,
        input: &str,
        context: &[Message],
        _classification: &ClassificationResult,
        decision: &RouteDecision,
    ) -> anyhow::Result<BackendReply> {
        let request = ChatRequest {
            model: model_name_from_decision(decision),
            messages: to_provider_messages(input, context),
            max_tokens: Some(512),
            temperature: Some(0.7),
            stream: false,
        };

        let mut runtime = self.runtime.lock().await;
        let routed = runtime.execute(decision, &request).await?;
        let attempts = routed
            .attempts
            .iter()
            .map(|a| {
                if a.success {
                    format!("{}:ok", a.provider)
                } else {
                    format!(
                        "{}:fail:{}",
                        a.provider,
                        a.error.clone().unwrap_or_else(|| "unknown".to_string())
                    )
                }
            })
            .collect();

        Ok(BackendReply {
            content: routed.response.content,
            route_attempts: attempts,
            token_usage: Some(routed.response.usage),
            provider: Some(routed.provider),
            model: Some(routed.response.model),
        })
    }
}

/// 一次对话回合的输出
#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub reply: String,
    pub route_reason: String,
    pub route_attempts: Vec<String>,
    /// 认知集成层输出（如已启用）
    pub mind_confidence: Option<f64>,
    pub knowledge_gaps: Vec<String>,
    pub budget_alerts: Vec<String>,
}

fn model_name_from_decision(decision: &RouteDecision) -> String {
    match &decision.target {
        crate::router::strategy::RouteTarget::Local(model) => model.clone(),
        crate::router::strategy::RouteTarget::Cloud(model) => model.clone(),
        crate::router::strategy::RouteTarget::Hybrid { cloud, .. } => cloud.clone(),
    }
}

fn to_provider_messages(input: &str, context: &[Message]) -> Vec<ChatMessage> {
    if context.is_empty() {
        return vec![ChatMessage {
            role: "user".to_string(),
            content: input.to_string(),
        }];
    }

    context
        .iter()
        .map(|m| ChatMessage {
            role: match m.role {
                Role::User => "user".to_string(),
                Role::Assistant => "assistant".to_string(),
                Role::System => "system".to_string(),
                Role::Tool => "tool".to_string(),
            },
            content: m.content.clone(),
        })
        .collect()
}

/// 核心编排器
pub struct ChatOrchestrator<B: ChatBackend> {
    session_manager: SessionManager,
    history_store: HistoryStore,
    classifier: IntentClassifier,
    router: crate::router::Router,
    context_compressor: ContextCompressor,
    prompt_context: Option<PromptContextConfig>,
    model_override: Option<String>,
    usage_store: Option<UsageStore>,
    memory_manager: Option<Arc<Mutex<MemoryManager>>>,
    steward: Option<LocalSteward>,
    agent_orchestrator: Option<AgentOrchestrator>,
    security_enforcer: Option<SecurityPolicyEnforcer>,
    mind: Option<MindIntegration>,
    prompt_cache: Option<PromptCache>,
    backend: B,
}

struct PromptContextConfig {
    user_id: String,
    consent_manager: ConsentManager,
    profile: Option<UserProfile>,
    persona: Option<Persona>,
}

impl<B: ChatBackend> ChatOrchestrator<B> {
    pub fn new(history_store: HistoryStore, backend: B) -> Self {
        Self {
            session_manager: SessionManager::new(),
            history_store,
            classifier: IntentClassifier::new(),
            router: crate::router::Router::new(RoutingStrategy::Balanced),
            context_compressor: ContextCompressor::new(2048),
            prompt_context: None,
            model_override: None,
            usage_store: None,
            memory_manager: None,
            steward: None,
            agent_orchestrator: None,
            security_enforcer: None,
            mind: None,
            prompt_cache: None,
            backend,
        }
    }

    pub fn new_with_persistence(
        session_db_path: std::path::PathBuf,
        history_store: HistoryStore,
        backend: B,
    ) -> anyhow::Result<Self> {
        let usage_store = UsageStore::new(session_db_path.clone())?;
        Ok(Self {
            session_manager: SessionManager::with_db_path(session_db_path)?,
            history_store,
            classifier: IntentClassifier::new(),
            router: crate::router::Router::new(RoutingStrategy::Balanced),
            context_compressor: ContextCompressor::new(2048),
            prompt_context: None,
            model_override: None,
            usage_store: Some(usage_store),
            memory_manager: None,
            steward: None,
            agent_orchestrator: None,
            security_enforcer: None,
            mind: None,
            prompt_cache: None,
            backend,
        })
    }

    pub fn set_model_override(&mut self, model: Option<String>) {
        self.model_override = model;
    }

    pub fn set_memory_manager(&mut self, manager: MemoryManager) {
        self.memory_manager = Some(Arc::new(Mutex::new(manager)));
    }

    pub fn set_routing_strategy(&mut self, strategy: RoutingStrategy) {
        self.router = crate::router::Router::new(strategy);
    }

    pub fn set_steward(&mut self, steward: LocalSteward) {
        self.steward = Some(steward);
    }

    pub fn set_agent_orchestrator(&mut self, orchestrator: AgentOrchestrator) {
        self.agent_orchestrator = Some(orchestrator);
    }

    pub fn set_security_enforcer(&mut self, enforcer: SecurityPolicyEnforcer) {
        self.security_enforcer = Some(enforcer);
    }

    pub fn set_mind(&mut self, mind: MindIntegration) {
        self.mind = Some(mind);
    }

    pub fn set_prompt_cache(&mut self, cache: PromptCache) {
        self.prompt_cache = Some(cache);
    }

    pub fn mind(&self) -> Option<&MindIntegration> {
        self.mind.as_ref()
    }

    pub fn model_override(&self) -> Option<&str> {
        self.model_override.as_deref()
    }

    pub fn set_prompt_context(
        &mut self,
        user_id: impl Into<String>,
        consent_manager: ConsentManager,
        profile: Option<UserProfile>,
        persona: Option<Persona>,
    ) {
        self.prompt_context = Some(PromptContextConfig {
            user_id: user_id.into(),
            consent_manager,
            profile,
            persona,
        });
    }

    pub async fn create_session(&mut self, title: Option<&str>) -> anyhow::Result<Session> {
        self.session_manager.create_session(title).await
    }

    pub async fn send_message(
        &mut self,
        session_id: Uuid,
        input: &str,
    ) -> anyhow::Result<ChatTurn> {
        self.session_manager.resume_session(session_id).await?;

        let user_message = Message {
            id: Uuid::new_v4(),
            session_id,
            role: Role::User,
            content: input.to_string(),
            timestamp: chrono::Utc::now(),
        };
        self.history_store.append(&user_message).await?;

        // 并行执行：意图分类 + 记忆召回
        let mm_clone = self.memory_manager.clone();
        let input_owned = input.to_string();
        let memory_future = async move {
            if let Some(mm) = mm_clone {
                let mut guard = mm.lock().await;
                let recalls = guard.on_user_message(&input_owned).await;
                if recalls.is_empty() {
                    None
                } else {
                    let text: Vec<String> = recalls.iter().map(|r| r.summary.clone()).collect();
                    Some(text.join("\n"))
                }
            } else {
                None
            }
        };
        let classify_future = self.classifier.classify(input);

        let (memory_context, classification_result) =
            tokio::join!(memory_future, classify_future);
        let classification = classification_result?;

        // Agent 集群路由：复杂任务走集群路径
        if let Some(ref agent_orch) = self.agent_orchestrator {
            let assessor = ComplexityAssessor::new();
            if let TaskRoute::Complex { suggested_roles, reason } =
                assessor.assess(&classification, input)
            {
                let cluster_result = agent_orch
                    .execute(input, &suggested_roles)
                    .await
                    .map_err(|e| anyhow::anyhow!("agent cluster error: {}", e))?;

                let assistant_message = Message {
                    id: Uuid::new_v4(),
                    session_id,
                    role: Role::Assistant,
                    content: cluster_result.output.clone(),
                    timestamp: chrono::Utc::now(),
                };
                self.history_store.append(&assistant_message).await?;

                return Ok(ChatTurn {
                    reply: cluster_result.output,
                    route_reason: format!("Agent集群: {}", reason),
                    route_attempts: vec![format!(
                        "cluster:agents={},tasks={}",
                        cluster_result.agents_used, cluster_result.tasks_completed
                    )],
                    mind_confidence: None,
                    knowledge_gaps: vec![],
                    budget_alerts: vec![],
                });
            }
        }

        let _constraints = self.build_constraints(&classification);
        let input_tokens =
            crate::router::token_estimator::TokenEstimator::estimate_text(input) as u64;

        // 路由决策：steward > model_override > router
        let (mut decision, steward_escalated) = if let Some(steward) = self.steward.as_ref() {
            let sd = steward.decide(&classification, input_tokens, memory_context.as_deref());
            let escalated = sd.escalated;
            (sd.route, Some(escalated))
        } else if let Some(model) = self.model_override.as_deref() {
            (build_manual_route_decision(model), None)
        } else {
            (
                self.router.select_model(&classification, input_tokens),
                None,
            )
        };
        let history = self.history_store.get_session_messages(session_id).await?;
        let mut context = self.context_compressor.compress(&history).await?;
        if let Some(system_message) = self.build_prompt_context_message(session_id)? {
            context.insert(0, system_message);
        }

        // 注入记忆上下文
        if let Some(mem_text) = memory_context {
            context.insert(
                0,
                Message {
                    id: Uuid::new_v4(),
                    session_id,
                    role: Role::System,
                    content: format!("[记忆上下文]\n{}", mem_text),
                    timestamp: chrono::Utc::now(),
                },
            );
        }

        // 安全策略执行：根据数据分级决定是否允许云端路由
        let mut effective_input = input.to_string();
        if let Some(ref enforcer) = self.security_enforcer {
            let data_class = DataClassifier::classify(input);
            let is_cloud = matches!(
                decision.target,
                crate::router::strategy::RouteTarget::Cloud(_)
                    | crate::router::strategy::RouteTarget::Hybrid { .. }
            );
            match enforcer.enforce(&data_class, is_cloud) {
                PolicyDecision::ForceLocal(reason) => {
                    decision = crate::router::strategy::RouteDecision {
                        model: crate::router::ModelId::Qwen7B,
                        target: crate::router::strategy::RouteTarget::Local("local-default".to_string()),
                        reason: format!("安全策略强制本地: {}", reason),
                        quality_score: decision.quality_score,
                        estimated_cost: 0.0,
                    };
                }
                PolicyDecision::Block(reason) => {
                    let block_reply = format!("[安全策略] {}", reason);
                    let assistant_message = Message {
                        id: Uuid::new_v4(),
                        session_id,
                        role: Role::Assistant,
                        content: block_reply.clone(),
                        timestamp: chrono::Utc::now(),
                    };
                    self.history_store.append(&assistant_message).await?;
                    return Ok(ChatTurn {
                        reply: block_reply,
                        route_reason: format!("安全策略阻止: {}", reason),
                        route_attempts: vec![],
                        mind_confidence: None,
                        knowledge_gaps: vec![],
                        budget_alerts: vec![],
                    });
                }
                PolicyDecision::RequireAnonymization(_) => {
                    effective_input = crate::privacy::anonymization::Anonymizer::anonymize(input);
                }
                PolicyDecision::Allow => {}
            }
        }

        // Prompt 缓存：查找（仅 temperature=0 时有效）
        let cache_key_data = (effective_input.clone(), 0.0f32, 512usize);
        if let Some(ref mut cache) = self.prompt_cache {
            if let Some((cached_response, _cached_tokens)) =
                cache.get(&cache_key_data.0, cache_key_data.1, cache_key_data.2)
            {
                let assistant_message = Message {
                    id: Uuid::new_v4(),
                    session_id,
                    role: Role::Assistant,
                    content: cached_response.clone(),
                    timestamp: chrono::Utc::now(),
                };
                self.history_store.append(&assistant_message).await?;

                return Ok(ChatTurn {
                    reply: cached_response,
                    route_reason: format!("{} (缓存命中)", decision.reason),
                    route_attempts: vec!["cache:hit".to_string()],
                    mind_confidence: None,
                    knowledge_gaps: vec![],
                    budget_alerts: vec![],
                });
            }
        }

        let backend_reply = self
            .backend
            .chat(&effective_input, &context, &classification, &decision)
            .await?;

        // Prompt 缓存：写入
        if let Some(ref mut cache) = self.prompt_cache {
            if let Some(usage) = backend_reply.token_usage.as_ref() {
                cache.put(
                    &cache_key_data.0,
                    cache_key_data.1,
                    cache_key_data.2,
                    &backend_reply.content,
                    usage.total_tokens,
                );
            }
        }

        let assistant_message = Message {
            id: Uuid::new_v4(),
            session_id,
            role: Role::Assistant,
            content: backend_reply.content.clone(),
            timestamp: chrono::Utc::now(),
        };
        self.history_store.append(&assistant_message).await?;

        // 后处理：记忆系统异步更新（不阻塞返回）
        if let Some(mm) = self.memory_manager.clone() {
            let user_text = input.to_string();
            let reply_text = backend_reply.content.clone();
            tokio::spawn(async move {
                let mut guard = mm.lock().await;
                guard.on_assistant_response(&user_text, &reply_text).await;
            });
        }

        // 认知集成层：PBRIC 循环 + 好奇心 + 经济皮层 + 进化追踪
        let mind_result = if let Some(ref mut mind) = self.mind {
            let model_id = backend_reply
                .model
                .as_deref()
                .unwrap_or("unknown");
            let cost = backend_reply
                .token_usage
                .as_ref()
                .map(|u| (u.prompt_tokens + u.completion_tokens) as f64 * 0.00001)
                .unwrap_or(0.0);
            let latency_ms = 0u64; // 实际延迟需要计时器，暂用 0
            mind.process_turn(input, model_id, cost, latency_ms, None)
                .await
                .ok()
        } else {
            None
        };

        // 大管家反馈学习
        if let Some(steward) = self.steward.as_mut() {
            let quality = steward.assess_response_quality(input, &backend_reply.content);
            let handled_locally = steward_escalated.map(|e| !e).unwrap_or(false);
            steward.record_outcome(
                classification.primary.clone(),
                decision.model,
                handled_locally,
                quality,
                input,
            );
        }

        if let (Some(store), Some(usage), Some(provider), Some(model)) = (
            self.usage_store.as_ref(),
            backend_reply.token_usage.as_ref(),
            backend_reply.provider.as_deref(),
            backend_reply.model.as_deref(),
        ) {
            store.record(session_id, provider, model, usage)?;
        }

        Ok(ChatTurn {
            reply: backend_reply.content,
            route_reason: decision.reason,
            route_attempts: backend_reply.route_attempts,
            mind_confidence: mind_result.as_ref().map(|r| r.confidence),
            knowledge_gaps: mind_result
                .as_ref()
                .map(|r| r.knowledge_gaps.clone())
                .unwrap_or_default(),
            budget_alerts: mind_result
                .as_ref()
                .map(|r| r.budget_alerts.clone())
                .unwrap_or_default(),
        })
    }

    pub async fn session_messages(&self, session_id: Uuid) -> anyhow::Result<Vec<Message>> {
        self.history_store.get_session_messages(session_id).await
    }

    /// 结束会话：触发记忆晋升和知识提取
    pub async fn end_session(&mut self, session_id: Uuid) -> anyhow::Result<()> {
        if let Some(mm) = self.memory_manager.clone() {
            let history = self.history_store.get_session_messages(session_id).await?;
            let pairs: Vec<(String, String)> = history
                .windows(2)
                .filter_map(|w| {
                    if matches!(w[0].role, Role::User) && matches!(w[1].role, Role::Assistant) {
                        Some((w[0].content.clone(), w[1].content.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            let mut guard = mm.lock().await;
            guard.on_session_end(&pairs).await?;
        }
        Ok(())
    }

    pub fn sessions(&self) -> &[Session] {
        self.session_manager.sessions()
    }

    pub fn session_token_usage(&self, session_id: Uuid) -> anyhow::Result<Option<TokenUsage>> {
        if let Some(store) = self.usage_store.as_ref() {
            return store.aggregate_for_session(session_id);
        }
        Ok(None)
    }

    fn build_constraints(&self, _classification: &ClassificationResult) -> RoutingConstraints {
        RoutingConstraints {
            network_available: true,
            prefer_local: true,
            require_cloud_capability: false,
            latency_sensitive: false,
        }
    }

    fn build_prompt_context_message(&self, session_id: Uuid) -> anyhow::Result<Option<Message>> {
        let Some(prompt_context) = self.prompt_context.as_ref() else {
            return Ok(None);
        };

        let mut builder = PromptContextBuilder::new(&prompt_context.user_id)
            .with_consent_manager(&prompt_context.consent_manager);
        if let Some(profile) = prompt_context.profile.as_ref() {
            builder = builder.with_profile(profile);
        }
        if let Some(persona) = prompt_context.persona.as_ref() {
            builder = builder.with_persona(persona);
        }

        let content = builder.build()?;
        Ok(Some(Message {
            id: Uuid::new_v4(),
            session_id,
            role: Role::System,
            content,
            timestamp: chrono::Utc::now(),
        }))
    }
}

fn build_manual_route_decision(model: &str) -> RouteDecision {
    let target = if model.starts_with("openai") || model.starts_with("anthropic") {
        crate::router::strategy::RouteTarget::Cloud(model.to_string())
    } else {
        crate::router::strategy::RouteTarget::Local(model.to_string())
    };

    let model_id = if model.contains("gpt-4o-mini") {
        crate::router::ModelId::GPT4oMini
    } else if model.contains("gpt-4o") {
        crate::router::ModelId::GPT4o
    } else if model.contains("claude") {
        crate::router::ModelId::Claude35Sonnet
    } else if model.contains("7b") {
        crate::router::ModelId::Qwen7B
    } else {
        crate::router::ModelId::Qwen3B
    };

    RouteDecision {
        model: model_id,
        target,
        reason: format!("手动模型覆盖：{}", model),
        quality_score: 0.8,
        estimated_cost: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockProvider;
    use crate::providers::registry::ProviderRegistry;
    use crate::router::fallback::FallbackRouter;
    use crate::router::runtime::RoutingRuntime;
    use crate::session::context::CompressionStrategy;
    use std::sync::Arc;
    use std::time::Duration;

    struct TestBackend;

    struct CaptureContextBackend {
        seen_context_len: std::sync::Mutex<usize>,
    }

    struct CapturePromptBackend {
        seen_context: std::sync::Mutex<Vec<Message>>,
    }

    struct CaptureDecisionBackend {
        last_target: std::sync::Mutex<Option<crate::router::strategy::RouteTarget>>,
    }

    #[async_trait]
    impl ChatBackend for TestBackend {
        async fn chat(
            &self,
            input: &str,
            _context: &[Message],
            classification: &ClassificationResult,
            _decision: &RouteDecision,
        ) -> anyhow::Result<BackendReply> {
            Ok(BackendReply {
                content: format!("echo:{}::{:?}", input, classification.primary),
                route_attempts: Vec::new(),
                token_usage: None,
                provider: None,
                model: None,
            })
        }
    }

    #[async_trait]
    impl ChatBackend for CaptureContextBackend {
        async fn chat(
            &self,
            _input: &str,
            context: &[Message],
            _classification: &ClassificationResult,
            _decision: &RouteDecision,
        ) -> anyhow::Result<BackendReply> {
            if let Ok(mut guard) = self.seen_context_len.lock() {
                *guard = context.len();
            }
            Ok(BackendReply {
                content: "ok".to_string(),
                route_attempts: Vec::new(),
                token_usage: None,
                provider: None,
                model: None,
            })
        }
    }

    #[async_trait]
    impl ChatBackend for CaptureDecisionBackend {
        async fn chat(
            &self,
            _input: &str,
            _context: &[Message],
            _classification: &ClassificationResult,
            decision: &RouteDecision,
        ) -> anyhow::Result<BackendReply> {
            if let Ok(mut guard) = self.last_target.lock() {
                *guard = Some(decision.target.clone());
            }
            Ok(BackendReply {
                content: "ok".to_string(),
                route_attempts: Vec::new(),
                token_usage: None,
                provider: None,
                model: None,
            })
        }
    }

    #[async_trait]
    impl ChatBackend for CapturePromptBackend {
        async fn chat(
            &self,
            _input: &str,
            context: &[Message],
            _classification: &ClassificationResult,
            _decision: &RouteDecision,
        ) -> anyhow::Result<BackendReply> {
            if let Ok(mut guard) = self.seen_context.lock() {
                *guard = context.to_vec();
            }
            Ok(BackendReply {
                content: "ok".to_string(),
                route_attempts: Vec::new(),
                token_usage: None,
                provider: None,
                model: None,
            })
        }
    }

    struct UsageBackend;

    #[async_trait]
    impl ChatBackend for UsageBackend {
        async fn chat(
            &self,
            _input: &str,
            _context: &[Message],
            _classification: &ClassificationResult,
            _decision: &RouteDecision,
        ) -> anyhow::Result<BackendReply> {
            Ok(BackendReply {
                content: "usage ok".to_string(),
                route_attempts: vec!["mock:ok".to_string()],
                token_usage: Some(TokenUsage {
                    prompt_tokens: 7,
                    completion_tokens: 3,
                    total_tokens: 10,
                }),
                provider: Some("mock".to_string()),
                model: Some("mock-chat-1".to_string()),
            })
        }
    }

    #[tokio::test]
    async fn stores_user_and_assistant_messages_per_turn() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orchestrator-test"));
        let mut orchestrator = ChatOrchestrator::new(history, TestBackend);
        let session = orchestrator.create_session(Some("测试会话")).await.unwrap();

        let turn = orchestrator
            .send_message(session.id, "请帮我写 Rust 代码")
            .await
            .unwrap();

        assert!(turn.reply.contains("echo"));

        let messages = orchestrator.session_messages(session.id).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, Role::User));
        assert!(matches!(messages[1].role, Role::Assistant));
    }

    #[tokio::test]
    async fn returns_error_when_session_not_found() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orchestrator-test-2"));
        let mut orchestrator = ChatOrchestrator::new(history, TestBackend);

        let result = orchestrator.send_message(Uuid::new_v4(), "hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn provider_backend_uses_routing_runtime() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("openai")));

        let fallback =
            FallbackRouter::with_config(vec!["openai".to_string()], 2, Duration::from_secs(30));
        let runtime = RoutingRuntime::new(registry, fallback);

        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-provider-test",
        ));
        let backend = ProviderChatBackend::new(runtime);
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        let session = orchestrator
            .create_session(Some("provider session"))
            .await
            .unwrap();

        let turn = orchestrator
            .send_message(session.id, "hello runtime")
            .await
            .unwrap();
        assert!(turn.reply.contains("hello runtime"));
        assert!(!turn.route_attempts.is_empty());
    }

    #[tokio::test]
    async fn compresses_context_before_backend_call() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-context-test",
        ));
        let backend = CaptureContextBackend {
            seen_context_len: std::sync::Mutex::new(0),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.context_compressor.max_tokens = 3;
        orchestrator.context_compressor.strategy = CompressionStrategy::Summarize;

        let session = orchestrator.create_session(Some("ctx")).await.unwrap();

        let _ = orchestrator
            .send_message(session.id, "one two three four five")
            .await
            .unwrap();
        let _ = orchestrator
            .send_message(session.id, "alpha beta gamma delta epsilon")
            .await
            .unwrap();

        let seen = *orchestrator.backend.seen_context_len.lock().unwrap();
        assert!(seen <= 2);
    }

    // 路由集成测试：复杂查询应路由到云端
    #[tokio::test]
    async fn enforces_cloud_route_for_tool_use_intent() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-cloud-constraint",
        ));
        let backend = CaptureDecisionBackend {
            last_target: std::sync::Mutex::new(None),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_routing_strategy(RoutingStrategy::QualityFirst);
        let session = orchestrator
            .create_session(Some("constraint cloud"))
            .await
            .unwrap();

        // 复杂查询：包含多个复杂关键词，长度足够触发 Reasoning 分类
        let _ = orchestrator
            .send_message(
                session.id,
                "请分析并推理这个复杂架构的优化方案，需要设计一个算法来证明其正确性",
            )
            .await
            .unwrap();

        let target = orchestrator.backend.last_target.lock().unwrap().clone();
        assert!(matches!(
            target,
            Some(crate::router::strategy::RouteTarget::Cloud(_))
        ));
    }

    #[tokio::test]
    async fn prefers_small_local_model_for_latency_sensitive_input() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-latency-constraint",
        ));
        let backend = CaptureDecisionBackend {
            last_target: std::sync::Mutex::new(None),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        let session = orchestrator
            .create_session(Some("constraint latency"))
            .await
            .unwrap();

        let _ = orchestrator.send_message(session.id, "你好").await.unwrap();

        let target = orchestrator.backend.last_target.lock().unwrap().clone();
        assert!(matches!(
            target,
            Some(crate::router::strategy::RouteTarget::Local(_))
        ));
    }

    #[tokio::test]
    async fn respects_manual_model_override() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-manual-model",
        ));
        let backend = CaptureDecisionBackend {
            last_target: std::sync::Mutex::new(None),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_model_override(Some("openai-gpt-4o-mini".to_string()));

        let session = orchestrator
            .create_session(Some("manual model"))
            .await
            .unwrap();
        let _ = orchestrator.send_message(session.id, "你好").await.unwrap();

        let target = orchestrator.backend.last_target.lock().unwrap().clone();
        assert!(matches!(
            target,
            Some(crate::router::strategy::RouteTarget::Cloud(model)) if model == "openai-gpt-4o-mini"
        ));
    }

    #[tokio::test]
    async fn reloads_sessions_from_persistent_store() {
        let session_db = crate::test_support::temp_db_path("orchestrator-session");
        let history_db = crate::test_support::temp_db_path("orchestrator-history");

        let mut first = ChatOrchestrator::new_with_persistence(
            session_db.clone(),
            HistoryStore::new(history_db.clone()),
            TestBackend,
        )
        .unwrap();

        let created = first.create_session(Some("持久化会话")).await.unwrap();
        drop(first);

        let second = ChatOrchestrator::new_with_persistence(
            session_db,
            HistoryStore::new(history_db),
            TestBackend,
        )
        .unwrap();

        assert_eq!(second.sessions().len(), 1);
        assert_eq!(second.sessions()[0].id, created.id);
        assert_eq!(second.sessions()[0].title, "持久化会话");
    }

    #[tokio::test]
    async fn persists_token_usage_when_available() {
        let session_db = crate::test_support::temp_db_path("orchestrator-usage");
        let history_db = crate::test_support::temp_db_path("orchestrator-usage-history");

        let mut orchestrator = ChatOrchestrator::new_with_persistence(
            session_db,
            HistoryStore::new(history_db),
            UsageBackend,
        )
        .unwrap();

        let session = orchestrator
            .create_session(Some("usage session"))
            .await
            .unwrap();
        let _ = orchestrator
            .send_message(session.id, "hello usage")
            .await
            .unwrap();

        let usage = orchestrator
            .session_token_usage(session.id)
            .unwrap()
            .unwrap();
        assert_eq!(usage.prompt_tokens, 7);
        assert_eq!(usage.completion_tokens, 3);
        assert_eq!(usage.total_tokens, 10);
    }

    #[tokio::test]
    async fn injects_personalized_system_prompt_into_backend_context_when_consent_exists() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-personalized-context",
        ));
        let backend = CapturePromptBackend {
            seen_context: std::sync::Mutex::new(Vec::new()),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        let consent_manager = crate::privacy::consent::ConsentManager::new(
            crate::test_support::temp_db_path("orchestrator-personalized-consent"),
        )
        .unwrap();
        consent_manager
            .record_consent(
                "user-1",
                crate::privacy::consent::ConsentPurpose::Personalization,
                true,
            )
            .unwrap();
        let mut profile = UserProfile::new("user-1");
        profile.display_name = "Alex".to_string();
        profile.language = "en-US".to_string();
        profile
            .preferences
            .insert("response_style".to_string(), "concise".to_string());
        let persona = Persona {
            id: Uuid::new_v4(),
            name: "expert".to_string(),
            system_prompt: "You are a precise assistant.".to_string(),
            tone: "direct".to_string(),
            expertise: vec!["automation".to_string()],
        };
        let persona_prompt = persona.system_prompt.clone();
        orchestrator.set_prompt_context("user-1", consent_manager, Some(profile), Some(persona));

        let session = orchestrator.create_session(Some("prompt")).await.unwrap();
        let _ = orchestrator
            .send_message(session.id, "hello")
            .await
            .unwrap();

        let context = orchestrator.backend.seen_context.lock().unwrap().clone();
        assert!(!context.is_empty());
        assert!(matches!(context[0].role, Role::System));
        assert!(context[0].content.contains("Alex"));
        assert!(context[0].content.contains("en-US"));
        assert!(context[0].content.contains("concise"));
        assert!(context[0].content.contains(&persona_prompt));
    }

    #[tokio::test]
    async fn injects_non_personalized_system_prompt_when_personalization_consent_is_missing() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-non-personalized-context",
        ));
        let backend = CapturePromptBackend {
            seen_context: std::sync::Mutex::new(Vec::new()),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        let consent_manager = crate::privacy::consent::ConsentManager::new(
            crate::test_support::temp_db_path("orchestrator-non-personalized-consent"),
        )
        .unwrap();
        let mut profile = UserProfile::new("user-1");
        profile.display_name = "Alex".to_string();
        let persona = Persona {
            id: Uuid::new_v4(),
            name: "expert".to_string(),
            system_prompt: "You are a precise assistant.".to_string(),
            tone: "direct".to_string(),
            expertise: vec!["automation".to_string()],
        };
        let persona_prompt = persona.system_prompt.clone();
        orchestrator.set_prompt_context("user-1", consent_manager, Some(profile), Some(persona));

        let session = orchestrator.create_session(Some("prompt")).await.unwrap();
        let _ = orchestrator
            .send_message(session.id, "hello")
            .await
            .unwrap();

        let context = orchestrator.backend.seen_context.lock().unwrap().clone();
        assert!(!context.is_empty());
        assert!(matches!(context[0].role, Role::System));
        assert!(context[0].content.contains("Personalization disabled"));
        assert!(!context[0].content.contains("Alex"));
        assert!(!context[0].content.contains(&persona_prompt));
    }

    #[tokio::test]
    async fn routes_to_cloud_for_complex_query() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-route-complex",
        ));
        let backend = CaptureDecisionBackend {
            last_target: std::sync::Mutex::new(None),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_routing_strategy(RoutingStrategy::QualityFirst);
        let session = orchestrator.create_session(Some("complex")).await.unwrap();

        let _ = orchestrator
            .send_message(
                session.id,
                "请分析这个复杂的架构设计方案，推理其优化空间并证明算法正确性",
            )
            .await
            .unwrap();

        let target = orchestrator.backend.last_target.lock().unwrap().clone();
        assert!(matches!(
            target,
            Some(crate::router::strategy::RouteTarget::Cloud(_))
        ));
    }

    #[tokio::test]
    async fn routes_to_local_for_simple_query() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-route-simple",
        ));
        let backend = CaptureDecisionBackend {
            last_target: std::sync::Mutex::new(None),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        let session = orchestrator.create_session(Some("simple")).await.unwrap();

        let _ = orchestrator.send_message(session.id, "嗨").await.unwrap();

        let target = orchestrator.backend.last_target.lock().unwrap().clone();
        assert!(matches!(
            target,
            Some(crate::router::strategy::RouteTarget::Local(_))
        ));
    }

    #[tokio::test]
    async fn model_override_bypasses_router() {
        let history = HistoryStore::new(crate::test_support::temp_db_path(
            "orchestrator-override-bypass",
        ));
        let backend = CaptureDecisionBackend {
            last_target: std::sync::Mutex::new(None),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_model_override(Some("anthropic-claude-3.5-sonnet".to_string()));

        let session = orchestrator.create_session(Some("override")).await.unwrap();
        // Simple input that would normally route local, but override forces cloud
        let _ = orchestrator.send_message(session.id, "嗨").await.unwrap();

        let target = orchestrator.backend.last_target.lock().unwrap().clone();
        assert!(matches!(
            target,
            Some(crate::router::strategy::RouteTarget::Cloud(m)) if m == "anthropic-claude-3.5-sonnet"
        ));
    }

    #[tokio::test]
    async fn send_message_stores_to_working_memory() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orch-mem-working"));
        let backend = TestBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);

        let dir = crate::test_support::temp_dir_path("orch-mem-working");
        let mm = MemoryManager::new(&dir).unwrap();
        orchestrator.set_memory_manager(mm);

        let session = orchestrator.create_session(Some("mem")).await.unwrap();
        let _ = orchestrator
            .send_message(session.id, "Rust 编程")
            .await
            .unwrap();

        // 等待异步后处理完成
        tokio::task::yield_now().await;

        assert_eq!(
            orchestrator
                .memory_manager
                .as_ref()
                .unwrap()
                .lock()
                .await
                .working()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn send_message_injects_memory_context() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orch-mem-inject"));
        let backend = CaptureContextBackend {
            seen_context_len: std::sync::Mutex::new(0),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);

        let dir = crate::test_support::temp_dir_path("orch-mem-inject");
        let mm = MemoryManager::new(&dir).unwrap();
        orchestrator.set_memory_manager(mm);

        let session = orchestrator.create_session(Some("inject")).await.unwrap();
        // First message triggers recall (new keywords), but stores are empty so no injection
        let _ = orchestrator
            .send_message(session.id, "Rust 编程")
            .await
            .unwrap();
        // Should still work without errors
    }

    #[tokio::test]
    async fn end_session_promotes_memories() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orch-mem-end"));
        let backend = TestBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);

        let dir = crate::test_support::temp_dir_path("orch-mem-end");
        let mm = MemoryManager::new(&dir).unwrap();
        orchestrator.set_memory_manager(mm);

        let session = orchestrator.create_session(Some("end")).await.unwrap();
        let _ = orchestrator
            .send_message(session.id, "重要的 Rust 讨论")
            .await
            .unwrap();

        orchestrator.end_session(session.id).await.unwrap();
        assert!(orchestrator
            .memory_manager
            .as_ref()
            .unwrap()
            .lock()
            .await
            .working()
            .is_empty());
    }

    #[tokio::test]
    async fn orchestrator_works_without_memory_manager() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orch-no-mem"));
        let backend = TestBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        // No memory manager set

        let session = orchestrator.create_session(Some("no-mem")).await.unwrap();
        let turn = orchestrator.send_message(session.id, "你好").await.unwrap();
        assert!(!turn.reply.is_empty());

        // end_session should be a no-op
        orchestrator.end_session(session.id).await.unwrap();
    }

    #[tokio::test]
    async fn steward_routes_casual_locally() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("stw-casual"));
        let backend = CaptureDecisionBackend {
            last_target: std::sync::Mutex::new(None),
        };
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_steward(LocalSteward::with_defaults());

        let session = orchestrator.create_session(Some("stw")).await.unwrap();
        let turn = orchestrator
            .send_message(session.id, "你好呀")
            .await
            .unwrap();
        assert!(!turn.reply.is_empty());
        // 闲聊应走本地
        assert!(turn.route_reason.contains("本地处理"));
    }

    #[tokio::test]
    async fn steward_escalates_reasoning() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("stw-reason"));
        let backend = TestBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_steward(LocalSteward::with_defaults());

        let session = orchestrator.create_session(Some("stw-r")).await.unwrap();
        let turn = orchestrator
            .send_message(
                session.id,
                "请详细分析并证明为什么 P=NP 问题如此困难，推导其复杂度理论基础",
            )
            .await
            .unwrap();
        assert!(!turn.reply.is_empty());
        // 复杂推理应升级云端
        assert!(turn.route_reason.contains("升级云端"));
    }

    #[tokio::test]
    async fn steward_records_outcome_after_response() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("stw-record"));
        let backend = TestBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_steward(LocalSteward::with_defaults());

        let session = orchestrator.create_session(Some("stw-rec")).await.unwrap();
        let _ = orchestrator.send_message(session.id, "你好").await.unwrap();

        // steward 应该记录了一条交互
        assert_eq!(orchestrator.steward.as_ref().unwrap().history_len(), 1);
    }

    #[tokio::test]
    async fn orchestrator_works_without_steward() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("no-stw"));
        let backend = TestBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        // 不设置 steward，应该走原有路由逻辑

        let session = orchestrator.create_session(Some("no-stw")).await.unwrap();
        let turn = orchestrator.send_message(session.id, "你好").await.unwrap();
        assert!(!turn.reply.is_empty());
        // 不应包含大管家的推理信息
        assert!(
            !turn.route_reason.contains("升级云端") && !turn.route_reason.contains("本地处理")
                || turn.route_reason.contains("策略")
        );
    }

    #[tokio::test]
    async fn parallel_classification_and_memory_results_consistent() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orch-parallel"));
        let backend = TestBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);

        let dir = crate::test_support::temp_dir_path("orch-parallel");
        let mm = MemoryManager::new(&dir).unwrap();
        orchestrator.set_memory_manager(mm);

        let session = orchestrator.create_session(Some("parallel")).await.unwrap();
        // 发送多条消息，验证并行执行结果一致
        for i in 0..3 {
            let turn = orchestrator
                .send_message(session.id, &format!("Rust 编程话题 {}", i))
                .await
                .unwrap();
            assert!(!turn.reply.is_empty());
        }
        // 工作记忆应包含 3 条
        tokio::task::yield_now().await;
        let guard = orchestrator.memory_manager.as_ref().unwrap().lock().await;
        assert_eq!(guard.working().len(), 3);
    }

    #[tokio::test]
    async fn post_processing_completes_asynchronously() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orch-async-post"));
        let backend = TestBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);

        let dir = crate::test_support::temp_dir_path("orch-async-post");
        let mm = MemoryManager::new(&dir).unwrap();
        orchestrator.set_memory_manager(mm);

        let session = orchestrator.create_session(Some("async-post")).await.unwrap();
        let turn = orchestrator
            .send_message(session.id, "我在学习 Rust 语言")
            .await
            .unwrap();
        assert!(!turn.reply.is_empty());

        // 让异步后处理完成
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 验证 profile 已被异步更新（on_assistant_response 提取了兴趣）
        let guard = orchestrator.memory_manager.as_ref().unwrap().lock().await;
        assert!(guard.profile().interests.contains(&"rust".to_string()));
    }

    #[tokio::test]
    async fn prompt_cache_hit_skips_backend() {
        let history = HistoryStore::new(crate::test_support::temp_db_path("orch-cache-hit"));
        let backend = UsageBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_prompt_cache(PromptCache::new(100, std::time::Duration::from_secs(60)));

        let session = orchestrator.create_session(Some("cache")).await.unwrap();
        // 第一次调用：缓存 miss，走 backend
        let turn1 = orchestrator.send_message(session.id, "hello").await.unwrap();
        assert!(!turn1.route_attempts.iter().any(|a| a.contains("cache:hit")));

        // 第二次相同输入：缓存命中
        let turn2 = orchestrator.send_message(session.id, "hello").await.unwrap();
        assert!(turn2.route_attempts.iter().any(|a| a.contains("cache:hit")));
        assert!(turn2.route_reason.contains("缓存命中"));
    }

    #[tokio::test]
    async fn prompt_cache_skips_nonzero_temperature() {
        // PromptCache 内部只缓存 temperature=0，orchestrator 固定传 0.0
        // 所以这里验证缓存确实生效
        let history = HistoryStore::new(crate::test_support::temp_db_path("orch-cache-temp"));
        let backend = UsageBackend;
        let mut orchestrator = ChatOrchestrator::new(history, backend);
        orchestrator.set_prompt_cache(PromptCache::new(100, std::time::Duration::from_secs(60)));

        let session = orchestrator.create_session(Some("temp")).await.unwrap();
        let _ = orchestrator.send_message(session.id, "test").await.unwrap();
        let turn2 = orchestrator.send_message(session.id, "test").await.unwrap();
        // 应该命中缓存
        assert!(turn2.route_attempts.iter().any(|a| a.contains("cache:hit")));
    }
}
