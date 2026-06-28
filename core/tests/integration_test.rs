//! 集成测试 - 完整对话流程
//!
//! 测试各模块协同工作：
//! - 智能路由（意图分类、模型选择）
//! - 记忆系统（工作记忆、情景记忆、知识库）
//! - Daemon Mind（预测、信念、反思）
//! - 经济皮层（成本追踪、ROI 优化）

use chrono::Utc;
use prometheus_core::memory::episodic::EpisodicMemory;
use prometheus_core::memory::working::WorkingMemory;
use prometheus_core::mind::belief::{BeliefModule, Evidence};
use prometheus_core::mind::economics::{BudgetConfig, CostRecord, EconomicCortex};
use prometheus_core::mind::predict::{PredictModule, UserAction};
use prometheus_core::mind::reflection::ReflectionModule;
use prometheus_core::router::classifier::{IntentClassifier, IntentType};
use prometheus_core::router::strategy::{Router, RoutingStrategy};

#[tokio::test]
async fn test_complete_conversation_flow() {
    // 1. 初始化所有模块
    let classifier = IntentClassifier::new();
    let mut router = Router::new(RoutingStrategy::Balanced);

    let mut working_memory = WorkingMemory::new(10);
    let episodic_db = prometheus_core::test_support::temp_db_path("integration-test-episodic");
    let _episodic_memory = EpisodicMemory::new(episodic_db, 128).unwrap();

    let mut predict_module = PredictModule::new();
    let mut belief_module = BeliefModule::new();
    let mut reflection_module = ReflectionModule::new();
    let mut economic_cortex = EconomicCortex::new(BudgetConfig::default());

    // 2. 模拟用户对话
    let user_message = "帮我写一个 Rust 函数来计算斐波那契数列";

    // 3. 意图分类
    let classification = classifier.classify(user_message).await.unwrap();
    assert!(matches!(
        classification.primary,
        IntentType::CodeGen | IntentType::ToolUse
    ));

    // 4. 路由决策
    let route_decision = router.select_model(&classification, 1000);
    // 验证路由决策有效
    assert!(route_decision.quality_score > 0.0);

    // 5. 工作记忆管理
    working_memory.add(user_message.to_string(), 0.8);
    let context = working_memory.get_active_context(10);
    assert!(!context.is_empty());

    // 6. 预测用户行为
    predict_module.record_action(UserAction::RequestCode);
    let predictions = predict_module.predict_next_action();
    assert!(!predictions.is_empty());

    // 7. 信念更新
    belief_module.update_belief(Evidence::UserFeedback {
        preference: "language".to_string(),
        value: 0.9,
    });
    assert!(belief_module.get_confidence() > 0.0);

    // 8. 反思对话质量
    let messages = vec![
        user_message.to_string(),
        "好的，我来帮你写".to_string(),
        "这是代码...".to_string(),
    ];
    let reflection = reflection_module
        .reflect_on_conversation(&messages, Some(0.85))
        .await;
    assert!(reflection.quality_score > 0.0);

    // 9. 成本追踪
    let cost_record = CostRecord {
        timestamp: Utc::now(),
        model_id: format!("{:?}", route_decision.model),
        input_tokens: 50,
        output_tokens: 200,
        cost: 0.05,
        latency_ms: 1200,
        satisfaction: Some(0.85),
    };
    economic_cortex.record_cost(cost_record);
    assert!(economic_cortex.get_daily_cost() > 0.0);

    // 10. ROI 优化
    let best_model = economic_cortex.get_best_roi_model();
    assert!(best_model.is_some());
}

#[tokio::test]
async fn test_memory_integration() {
    use prometheus_core::memory::episodic::{Message, Role};

    // 测试记忆系统集成
    let episodic_db = prometheus_core::test_support::temp_db_path("integration-test-memory");
    let mut episodic_memory = EpisodicMemory::new(episodic_db, 128).unwrap();

    // 添加对话
    let messages = vec![
        Message {
            role: Role::User,
            content: "什么是 Rust？".to_string(),
        },
        Message {
            role: Role::Assistant,
            content: "Rust 是一门系统编程语言".to_string(),
        },
    ];

    episodic_memory.add_episode(messages).await.unwrap();

    // 搜索相关对话
    let results = episodic_memory.search("Rust 编程", 5).await.unwrap();
    assert!(!results.is_empty());
}

#[tokio::test]
async fn test_routing_with_cost_optimization() {
    // 测试路由与成本优化集成
    // 成本优化策略
    let mut router = Router::new(RoutingStrategy::CostOptimized);

    let classifier = IntentClassifier::new();
    let classification = classifier.classify("简单问题").await.unwrap();

    let decision = router.select_model(&classification, 500);

    // 成本优化应选择较便宜的模型
    assert!(decision.estimated_cost < 0.1);
}

#[test]
fn test_belief_and_prediction_integration() {
    // 测试信念与预测模块集成
    let mut belief_module = BeliefModule::new();
    let mut predict_module = PredictModule::new();

    // 建立用户偏好
    belief_module.update_belief(Evidence::UserFeedback {
        preference: "code_style".to_string(),
        value: 0.9,
    });

    // 记录行为模式
    for _ in 0..3 {
        predict_module.record_action(UserAction::RequestCode);
        predict_module.record_action(UserAction::AskQuestion);
    }

    // 预测应该反映模式
    let predictions = predict_module.predict_next_action();
    assert!(!predictions.is_empty());

    // 信念应该有置信度
    assert!(belief_module.get_confidence() > 0.0);
}

#[tokio::test]
async fn test_reflection_with_economics() {
    // 测试反思与经济皮层集成
    let mut reflection_module = ReflectionModule::new();
    let mut economic_cortex = EconomicCortex::default();

    // 模拟对话
    let messages = vec![
        "用户问题".to_string(),
        "助手回答".to_string(),
        "用户追问".to_string(),
        "助手详细回答".to_string(),
    ];

    let reflection = reflection_module
        .reflect_on_conversation(&messages, Some(0.8))
        .await;

    // 记录成本
    economic_cortex.record_cost(CostRecord {
        timestamp: Utc::now(),
        model_id: "gpt-4".to_string(),
        input_tokens: 100,
        output_tokens: 200,
        cost: 0.05,
        latency_ms: 1000,
        satisfaction: Some(reflection.quality_score),
    });

    // 质量分数应该影响 ROI
    let stats = economic_cortex.get_model_stats("gpt-4").unwrap();
    assert!(stats.roi_score > 0.0);
}

#[tokio::test]
async fn test_multi_turn_conversation() {
    // 测试多轮对话
    let mut working_memory = WorkingMemory::new(5);
    let mut reflection_module = ReflectionModule::new();
    let mut predict_module = PredictModule::new();

    let turns = vec![
        ("用户", "你好"),
        ("助手", "你好！有什么可以帮你的？"),
        ("用户", "帮我写代码"),
        ("助手", "好的，需要什么语言？"),
        ("用户", "Rust"),
        ("助手", "明白了，我来写 Rust 代码"),
    ];

    let mut messages = Vec::new();

    for (role, text) in turns {
        messages.push(text.to_string());

        if role == "用户" {
            working_memory.add(text.to_string(), 0.7);
            predict_module.record_action(UserAction::AskQuestion);
        }
    }

    // 工作记忆应该保留最近的消息
    let context = working_memory.get_active_context(10);
    assert!(context.len() <= 5);

    // 反思对话质量
    let reflection = reflection_module
        .reflect_on_conversation(&messages, Some(0.85))
        .await;
    assert!(reflection.quality_score > 0.6);

    // 预测下一步行为
    let predictions = predict_module.predict_next_action();
    assert!(!predictions.is_empty());
}
