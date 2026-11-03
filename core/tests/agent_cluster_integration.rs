//! Phase 7: Agent 集群端到端集成测试
//!
//! 验证 Agent 集群与 ChatOrchestrator 的完整集成：
//! 1. 简单请求走原有路径（向后兼容）
//! 2. 复杂请求触发 Agent 集群
//! 3. 完整端到端流程（Mock 模型）
//! 4. Agent 集群执行后团队正确解散
//! 5. 无 AgentOrchestrator 时行为不变
//! 6. 复杂度评估边界测试

use std::sync::Arc;

use async_trait::async_trait;
use prometheus_core::agent::execution::model_pool::{MockModelBackend, ModelPool, ModelTier};
use prometheus_core::agent::integration::{AgentOrchestrator, ComplexityAssessor, TaskRoute};
use prometheus_core::agent::types::Role;
use prometheus_core::orchestrator::{BackendReply, ChatBackend, ChatOrchestrator};
use prometheus_core::router::classifier::ClassificationResult;
use prometheus_core::router::strategy::RouteDecision;
use prometheus_core::session::history::{HistoryStore, Message};

struct TestBackend;

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

async fn make_model_pool() -> Arc<ModelPool> {
    let pool = Arc::new(ModelPool::new());
    pool.register(
        ModelTier::Standard,
        Arc::new(MockModelBackend::new("mock", "gpt-4", "ANSWER: 任务完成")),
    )
    .await;
    pool
}

// ============================================================================
// Test 1: 无 AgentOrchestrator 时行为不变（向后兼容）
// ============================================================================

#[tokio::test]
async fn orchestrator_without_cluster_works_normally() {
    let history = HistoryStore::new(prometheus_core::test_support::temp_db_path(
        "cluster-no-agent",
    ));
    let mut orchestrator = ChatOrchestrator::new(history, TestBackend);
    // 不设置 agent_orchestrator

    let session = orchestrator.create_session(Some("normal")).await.unwrap();
    let turn = orchestrator.send_message(session.id, "你好").await.unwrap();

    assert!(turn.reply.contains("echo"));
    assert!(!turn.route_reason.contains("Agent集群"));
}

// ============================================================================
// Test 2: 简单请求走原有路径
// ============================================================================

#[tokio::test]
async fn simple_request_bypasses_cluster() {
    let history = HistoryStore::new(prometheus_core::test_support::temp_db_path(
        "cluster-simple-bypass",
    ));
    let pool = make_model_pool().await;
    let mut orchestrator = ChatOrchestrator::new(history, TestBackend);
    orchestrator.set_agent_orchestrator(AgentOrchestrator::new(pool));

    let session = orchestrator.create_session(Some("simple")).await.unwrap();
    // "你好" 是简单闲聊，复杂度低，应走原有路径
    let turn = orchestrator.send_message(session.id, "你好").await.unwrap();

    assert!(turn.reply.contains("echo"));
    assert!(!turn.route_reason.contains("Agent集群"));
}

// ============================================================================
// Test 3: 复杂请求触发 Agent 集群
// ============================================================================

#[tokio::test]
async fn complex_request_triggers_cluster() {
    let history = HistoryStore::new(prometheus_core::test_support::temp_db_path(
        "cluster-complex-trigger",
    ));
    let pool = make_model_pool().await;
    let mut orchestrator = ChatOrchestrator::new(history, TestBackend);
    orchestrator.set_agent_orchestrator(AgentOrchestrator::new(pool));

    let session = orchestrator.create_session(Some("complex")).await.unwrap();
    // 包含多步骤关键词 "设计并实现"，应触发集群
    let turn = orchestrator
        .send_message(
            session.id,
            "设计并实现一个完整的用户认证系统，包含OAuth和JWT",
        )
        .await
        .unwrap();

    assert!(turn.route_reason.contains("Agent集群"));
    assert!(!turn.reply.is_empty());
}

// ============================================================================
// Test 4: Agent 集群执行后团队正确解散
// ============================================================================

#[tokio::test]
async fn cluster_disbands_after_completion() {
    let pool = make_model_pool().await;
    let orch = AgentOrchestrator::new(pool);

    let result = orch
        .execute("实现缓存系统", &[Role::Engineer, Role::QA])
        .await
        .unwrap();

    assert!(result.tasks_completed > 0);
    assert_eq!(orch.agent_count().await, 0, "团队应在执行后解散");
}

// ============================================================================
// Test 5: 完整端到端流程（Mock 模型）
// ============================================================================

#[tokio::test]
async fn full_e2e_with_mock_model() {
    let pool = make_model_pool().await;
    let orch = AgentOrchestrator::new(pool);

    let roles = vec![
        Role::ProductManager,
        Role::Architect,
        Role::Engineer,
        Role::QA,
        Role::Security,
    ];
    let result = orch
        .execute("设计并实现安全的用户认证系统", &roles)
        .await
        .unwrap();

    assert_eq!(result.tasks_completed, 5);
    assert_eq!(result.agents_used, 5);
    assert!(result.quality_passed);
    assert!(!result.output.is_empty());
    assert_eq!(orch.agent_count().await, 0);
}

// ============================================================================
// Test 6: 复杂度评估边界测试
// ============================================================================

#[tokio::test]
async fn complexity_assessment_boundary() {
    use prometheus_core::router::classifier::IntentType;

    let assessor = ComplexityAssessor::new(); // threshold = 7

    // 刚好在阈值以下
    let below = ClassificationResult {
        primary: IntentType::CodeGen,
        confidence: 0.8,
        complexity: 6,
    };
    assert!(matches!(
        assessor.assess(&below, "写一个函数"),
        TaskRoute::Simple { .. }
    ));

    // 刚好在阈值上
    let at = ClassificationResult {
        primary: IntentType::CodeGen,
        confidence: 0.8,
        complexity: 7,
    };
    assert!(matches!(
        assessor.assess(&at, "写一个函数"),
        TaskRoute::Complex { .. }
    ));

    // 低复杂度但有多步骤关键词
    let multi_step = ClassificationResult {
        primary: IntentType::CodeGen,
        confidence: 0.8,
        complexity: 3,
    };
    assert!(matches!(
        assessor.assess(&multi_step, "先分析需求然后实现"),
        TaskRoute::Complex { .. }
    ));
}

// ============================================================================
// Test 7: 集群结果存入历史记录
// ============================================================================

#[tokio::test]
async fn cluster_result_stored_in_history() {
    let history = HistoryStore::new(prometheus_core::test_support::temp_db_path(
        "cluster-history-store",
    ));
    let pool = make_model_pool().await;
    let mut orchestrator = ChatOrchestrator::new(history, TestBackend);
    orchestrator.set_agent_orchestrator(AgentOrchestrator::new(pool));

    let session = orchestrator.create_session(Some("history")).await.unwrap();
    let _ = orchestrator
        .send_message(session.id, "设计并实现分布式缓存")
        .await
        .unwrap();

    let messages = orchestrator.session_messages(session.id).await.unwrap();
    // 应有 user + assistant 两条消息
    assert_eq!(messages.len(), 2);
    assert!(matches!(
        messages[0].role,
        prometheus_core::session::history::Role::User
    ));
    assert!(matches!(
        messages[1].role,
        prometheus_core::session::history::Role::Assistant
    ));
}
