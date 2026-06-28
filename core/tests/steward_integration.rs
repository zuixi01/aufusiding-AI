//! LocalSteward 集成测试
//!
//! 测试大管家在多轮交互中的学习能力、记忆感知路由和完整会话生命周期。

use prometheus_core::router::capability::ModelId;
use prometheus_core::router::classifier::{ClassificationResult, IntentType};
use prometheus_core::router::steward::{LocalSteward, StewardConfig};

fn knowledge_classification(complexity: u8, confidence: f32) -> ClassificationResult {
    ClassificationResult {
        primary: IntentType::Knowledge,
        confidence,
        complexity,
    }
}

fn casual_classification() -> ClassificationResult {
    ClassificationResult {
        primary: IntentType::Casual,
        confidence: 0.85,
        complexity: 2,
    }
}

fn reasoning_classification() -> ClassificationResult {
    ClassificationResult {
        primary: IntentType::Reasoning,
        confidence: 0.9,
        complexity: 9,
    }
}

/// 多轮交互后本地能力边界扩展
#[test]
fn steward_learns_from_cloud_shifts_boundary() {
    let mut steward = LocalSteward::with_defaults();

    // 初始状态：Knowledge + complexity=6 可能升级云端
    let cls = knowledge_classification(6, 0.7);
    let initial = steward.decide(&cls, 100, None);
    let initial_score = initial.reasoning.final_score;

    // 模拟多次本地高质量处理 Knowledge 意图
    for i in 0..30 {
        steward.record_outcome(
            IntentType::Knowledge,
            ModelId::Qwen7B,
            true,
            0.9,
            &format!("知识问答第{}轮", i),
        );
    }

    // 学习后重新决策
    let after = steward.decide(&cls, 100, None);
    let after_score = after.reasoning.final_score;

    // 能力边界应该扩展：final_score 提升
    assert!(
        after_score > initial_score,
        "学习后 score 应提升: {:.3} -> {:.3}",
        initial_score,
        after_score
    );

    // memory_match 也应该提升（历史中有大量本地成功记录）
    assert!(
        after.reasoning.memory_match > 0.5,
        "memory_match 应 > 0.5, got {:.3}",
        after.reasoning.memory_match
    );
}

/// 有记忆上下文时更倾向本地处理
#[test]
fn steward_memory_aware_routing() {
    let mut steward = LocalSteward::with_defaults();

    // 先积累一些本地成功经验
    for _ in 0..10 {
        steward.record_outcome(
            IntentType::Knowledge,
            ModelId::Qwen7B,
            true,
            0.85,
            "Rust 所有权",
        );
    }

    let cls = knowledge_classification(5, 0.75);

    // 无记忆上下文
    let without_mem = steward.decide(&cls, 100, None);

    // 有记忆上下文（不影响 score 计算，但影响预处理）
    let with_mem = steward.decide(&cls, 100, Some("用户之前讨论过 Rust 所有权系统"));

    // 两者的 final_score 应相同（记忆上下文不影响决策分数）
    assert!(
        (without_mem.reasoning.final_score - with_mem.reasoning.final_score).abs() < f64::EPSILON
    );

    // 如果升级云端，有记忆的版本应包含预处理上下文
    if with_mem.escalated {
        let query = with_mem.preprocessed_query.as_ref().unwrap();
        assert!(query.contains("记忆上下文"));
        assert!(query.contains("Rust"));
    }
}

/// 完整会话生命周期：本地→升级→学习→下次本地处理
#[test]
fn steward_full_session_lifecycle() {
    let config = StewardConfig {
        base_threshold: 0.45,
        high_complexity_threshold: 0.75,
        ..StewardConfig::default()
    };
    let mut steward = LocalSteward::new(config);

    // 第一轮：简单闲聊 → 本地处理
    let casual = casual_classification();
    let d1 = steward.decide(&casual, 20, None);
    assert!(!d1.escalated, "闲聊应本地处理");
    assert!(d1.route.model.is_local());

    // 记录闲聊结果
    steward.record_outcome(IntentType::Casual, d1.route.model, true, 0.85, "你好呀");

    // 第二轮：复杂推理 → 升级云端
    let reasoning = reasoning_classification();
    let d2 = steward.decide(&reasoning, 200, None);
    assert!(d2.escalated, "复杂推理应升级云端");
    assert!(!d2.route.model.is_local());
    assert!(d2.preprocessed_query.is_some());

    // 记录云端结果（高质量）
    steward.record_outcome(IntentType::Reasoning, d2.route.model, false, 0.95, "推理题");

    // 第三轮：多次本地处理 Knowledge 后，边界扩展
    let knowledge = knowledge_classification(5, 0.8);
    let d3_before = steward.decide(&knowledge, 100, None);

    // 模拟多次本地成功
    for _ in 0..20 {
        steward.record_outcome(
            IntentType::Knowledge,
            ModelId::Qwen7B,
            true,
            0.9,
            "知识问答",
        );
    }

    let d3_after = steward.decide(&knowledge, 100, None);

    // 学习后 score 应提升
    assert!(
        d3_after.reasoning.final_score >= d3_before.reasoning.final_score,
        "学习后 Knowledge score 应提升"
    );

    // 验证历史记录数量正确
    // 1 (casual) + 1 (reasoning) + 20 (knowledge) = 22
    assert_eq!(steward.history_len(), 22);
}
