//! 端到端集成测试：ChatOrchestrator + MindIntegration 完整管线
//!
//! 验证认知集成层与编排器的协同工作。

use async_trait::async_trait;
use prometheus_core::mind::economics::BudgetConfig;
use prometheus_core::mind::integration::MindIntegration;
use prometheus_core::orchestrator::{BackendReply, ChatBackend, ChatOrchestrator};
use prometheus_core::providers::traits::TokenUsage;
use prometheus_core::router::classifier::ClassificationResult;
use prometheus_core::router::strategy::RouteDecision;
use prometheus_core::session::history::{HistoryStore, Message};
use tempfile::TempDir;

/// 带 token 统计的测试后端
struct MindTestBackend;

#[async_trait]
impl ChatBackend for MindTestBackend {
    async fn chat(
        &self,
        input: &str,
        _context: &[Message],
        _classification: &ClassificationResult,
        _decision: &RouteDecision,
    ) -> anyhow::Result<BackendReply> {
        Ok(BackendReply {
            content: format!("回复: {}", input),
            route_attempts: vec!["local:ok".to_string()],
            token_usage: Some(TokenUsage {
                prompt_tokens: 50,
                completion_tokens: 30,
                total_tokens: 80,
            }),
            provider: Some("local".to_string()),
            model: Some("qwen-7b".to_string()),
        })
    }
}

fn create_orchestrator_with_mind() -> (ChatOrchestrator<MindTestBackend>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("history.db");
    let history = HistoryStore::new(db_path);
    let mut orch = ChatOrchestrator::new(history, MindTestBackend);
    let mind = MindIntegration::new("e2e_test");
    orch.set_mind(mind);
    (orch, tmp)
}

// PLACEHOLDER_TESTS

#[tokio::test]
async fn e2e_single_turn_with_mind() {
    let (mut orch, _tmp) = create_orchestrator_with_mind();
    let session = orch.create_session(Some("e2e")).await.unwrap();
    let turn = orch.send_message(session.id, "你好").await.unwrap();

    assert!(turn.reply.contains("回复"));
    // mind 应该产生置信度
    assert!(turn.mind_confidence.is_some());
    let conf = turn.mind_confidence.unwrap();
    assert!(conf > 0.0 && conf <= 1.0);
}

#[tokio::test]
async fn e2e_multiple_turns_accumulate_mind_state() {
    let (mut orch, _tmp) = create_orchestrator_with_mind();
    let session = orch.create_session(Some("multi")).await.unwrap();

    for i in 0..3 {
        let turn = orch
            .send_message(session.id, &format!("第{}轮对话", i))
            .await
            .unwrap();
        assert!(turn.mind_confidence.is_some());
    }

    // mind 内部应该累积了 3 轮
    let mind = orch.mind().unwrap();
    assert_eq!(mind.turn_count(), 3);
    // 进化引擎应有 3 个事件
    assert_eq!(mind.evolution().events().len(), 3);
    // 经济皮层应有记录
    let stats = mind.economics().get_model_stats("qwen-7b");
    assert!(stats.is_some());
    assert_eq!(stats.unwrap().call_count, 3);
}

#[tokio::test]
async fn e2e_budget_alert_propagates_to_chat_turn() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("history.db");
    let history = HistoryStore::new(db_path);
    let mut orch = ChatOrchestrator::new(history, MindTestBackend);
    let budget = BudgetConfig {
        daily_budget: 0.0001,
        monthly_budget: 0.001,
        alert_threshold: 0.5,
    };
    let mind = MindIntegration::new("budget_e2e").with_budget(budget);
    orch.set_mind(mind);

    let session = orch.create_session(Some("budget")).await.unwrap();
    let turn = orch.send_message(session.id, "测试预算").await.unwrap();

    // 超预算应有告警传播到 ChatTurn
    assert!(!turn.budget_alerts.is_empty());
}

#[tokio::test]
async fn e2e_new_domain_detected_as_knowledge_gap() {
    let (mut orch, _tmp) = create_orchestrator_with_mind();
    let session = orch.create_session(Some("curiosity")).await.unwrap();
    let turn = orch
        .send_message(session.id, "量子纠缠的原理是什么")
        .await
        .unwrap();

    // 新领域应被好奇心引擎检测
    let mind = orch.mind().unwrap();
    assert!(mind.curiosity().gaps().len() > 0 || !turn.knowledge_gaps.is_empty());
}

#[tokio::test]
async fn e2e_coding_input_tracks_tool_use_dimension() {
    let (mut orch, _tmp) = create_orchestrator_with_mind();
    let session = orch.create_session(Some("code")).await.unwrap();
    orch.send_message(session.id, "帮我写一段代码实现排序")
        .await
        .unwrap();

    let mind = orch.mind().unwrap();
    let events = mind.evolution().events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].dimension,
        prometheus_core::mind::evolution::Dimension::ToolUse
    );
}

#[tokio::test]
async fn e2e_thought_log_records_all_turns() {
    let (mut orch, _tmp) = create_orchestrator_with_mind();
    let session = orch.create_session(Some("thoughts")).await.unwrap();

    orch.send_message(session.id, "你好").await.unwrap();
    orch.send_message(session.id, "帮我分析一下").await.unwrap();

    let mind = orch.mind().unwrap();
    // integration 层的 thought_logger 每轮记 1 条 Reflect
    // 2 轮 → 2 条
    assert!(mind.thought_logger().len() >= 2);
    // PBRIC 内部应有 2 条思维链
    assert_eq!(mind.pbric().thought_logger().chain_count(), 2);
}

#[tokio::test]
async fn e2e_without_mind_still_works() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("history.db");
    let history = HistoryStore::new(db_path);
    let mut orch = ChatOrchestrator::new(history, MindTestBackend);
    // 不设置 mind
    let session = orch.create_session(Some("no_mind")).await.unwrap();
    let turn = orch.send_message(session.id, "你好").await.unwrap();

    assert!(turn.reply.contains("回复"));
    assert!(turn.mind_confidence.is_none());
    assert!(turn.knowledge_gaps.is_empty());
    assert!(turn.budget_alerts.is_empty());
}
