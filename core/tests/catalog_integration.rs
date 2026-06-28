//! 模型目录 + 难度评估 + 大管家 V2 集成测试

use prometheus_core::router::capability::ModelId;
use prometheus_core::router::catalog::{CapabilityDomain, ModelCatalog, ModelTier};
use prometheus_core::router::classifier::{ClassificationResult, IntentType};
use prometheus_core::router::difficulty::{assess_difficulty, TaskDifficulty};
use prometheus_core::router::steward::LocalSteward;

fn make_cls(intent: IntentType, complexity: u8, confidence: f32) -> ClassificationResult {
    ClassificationResult {
        primary: intent,
        confidence,
        complexity,
    }
}

#[test]
fn full_routing_low_to_premium() {
    let steward = LocalSteward::with_defaults();

    // Low: 闲聊 → 本地
    let low = make_cls(IntentType::Casual, 2, 0.9);
    let d_low = steward.decide(&low, 50, None);
    assert!(!d_low.escalated, "Low 应本地处理");

    // Medium: 中等编程 → Standard 层（云端）
    let med = make_cls(IntentType::CodeGen, 5, 0.8);
    let d_med = steward.decide(&med, 100, None);
    assert!(d_med.escalated, "Medium 应升级");

    // High: 复杂推理 → Premium 层（云端）
    let high = make_cls(IntentType::Reasoning, 9, 0.9);
    let d_high = steward.decide(&high, 200, None);
    assert!(d_high.escalated, "High 应升级");
    assert!(!d_high.route.model.is_local());
}

#[test]
fn economic_efficiency_selects_cheaper() {
    let catalog = ModelCatalog::new();

    // Standard 层数学：deepseek-chat 最强且最便宜
    let best = catalog.best_for_domain(CapabilityDomain::Mathematics, ModelTier::Standard);
    assert!(best.is_some());
    let best = best.unwrap();
    assert_eq!(best.id, "deepseek-chat");
    assert!(best.input_price_per_m < 1.0, "DeepSeek 应该便宜");
}

#[test]
fn catalog_with_steward_lifecycle() {
    let mut steward = LocalSteward::with_defaults();

    // 1. 初始决策
    let cls = make_cls(IntentType::Knowledge, 4, 0.85);
    let d1 = steward.decide(&cls, 100, None);

    // 2. 记录多次本地成功
    for _ in 0..15 {
        steward.record_outcome(
            IntentType::Knowledge,
            ModelId::Qwen7B,
            true,
            0.9,
            "知识问答",
        );
    }

    // 3. 学习后再次决策
    let d2 = steward.decide(&cls, 100, None);
    assert!(
        d2.reasoning.final_score >= d1.reasoning.final_score,
        "学习后得分应提升"
    );
}

#[test]
fn multimodal_routes_to_correct_vendor() {
    use prometheus_core::router::catalog::Modality;

    let catalog = ModelCatalog::new();

    // 图像任务应路由到 MiniMax
    let images = catalog.by_modality(Modality::Image);
    assert!(!images.is_empty());
    assert_eq!(images[0].id, "minimax-image");

    // 视频任务应路由到 MiniMax
    let videos = catalog.by_modality(Modality::Video);
    assert!(!videos.is_empty());
    assert_eq!(videos[0].id, "minimax-video");
}

#[test]
fn difficulty_assessment_consistency() {
    // Low
    let low = make_cls(IntentType::Casual, 2, 0.9);
    let a_low = assess_difficulty(&low, "你好");
    assert_eq!(a_low.difficulty, TaskDifficulty::Low);

    // Medium
    let med = make_cls(IntentType::CodeGen, 5, 0.8);
    let a_med = assess_difficulty(&med, "写一个排序算法");
    assert!(a_med.difficulty >= TaskDifficulty::Medium);

    // High
    let high = make_cls(IntentType::Reasoning, 9, 0.95);
    let a_high = assess_difficulty(&high, "证明哥德巴赫猜想");
    assert_eq!(a_high.difficulty, TaskDifficulty::High);
}

#[test]
fn new_models_have_pricing() {
    use prometheus_core::router::cost::CostCalculator;

    let calc = CostCalculator::new();
    // 新模型应有定价（非零延迟估算）
    let new_models = [
        ModelId::QwenTurbo,
        ModelId::QwenMax,
        ModelId::DeepSeekChat,
        ModelId::ClaudeOpus,
        ModelId::GeminiFlash,
        ModelId::GeminiPro,
    ];
    for model in &new_models {
        let latency = calc.estimate_latency(*model, 100);
        assert!(latency > 0, "{:?} 应有延迟估算", model);
    }
}

#[test]
fn new_models_in_catalog() {
    let catalog = ModelCatalog::new();
    let new_ids = [
        "qwen-turbo",
        "qwen-max",
        "deepseek-chat",
        "claude-opus",
        "gemini-flash",
        "gemini-pro",
    ];
    for id in &new_ids {
        assert!(catalog.get(id).is_some(), "目录缺少: {}", id);
    }
}
