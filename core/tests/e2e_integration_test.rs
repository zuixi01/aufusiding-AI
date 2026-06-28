//! Phase 6: 端到端集成测试
//!
//! 验证完整对话流程中所有模块的协同工作：
//! 1. 完整对话管道（意图分类 → 路由 → LLM 推理 → 记忆 → 反思 → 成本追踪）
//! 2. 多轮对话（上下文传递、记忆衰减、情景记忆检索）
//! 3. LLM 模块集成（意图分类、反思质量、失败降级）
//! 4. 经济层（成本追踪、ROI 计算、模型推荐）
//! 5. 错误处理（Ollama 不可用、网络超时、优雅降级）

use chrono::Utc;
use prometheus_core::memory::embedding::{
    EmbeddingService, MockEmbeddingService, ProviderEmbeddingService,
};
use prometheus_core::memory::episodic::{EpisodicMemory, Message, Role};
use prometheus_core::memory::working::WorkingMemory;
use prometheus_core::mind::belief::{BeliefModule, Evidence};
use prometheus_core::mind::economics::{BudgetConfig, CostRecord, EconomicCortex};
use prometheus_core::mind::predict::{PredictModule, UserAction};
use prometheus_core::mind::reflection::ReflectionModule;
use prometheus_core::providers::ollama::OllamaProvider;
use prometheus_core::providers::traits::Provider;
use prometheus_core::router::capability::{CapabilityMatrix, ModelId};
use prometheus_core::router::classifier::{IntentClassifier, IntentType};
use prometheus_core::router::strategy::{Router, RoutingStrategy};
use std::sync::Arc;

// ============================================================================
// Test 1: 完整对话管道（使用 Mock Provider）
// ============================================================================

#[tokio::test]
async fn test_complete_conversation_pipeline() {
    // 1. 初始化所有模块
    let classifier = IntentClassifier::new();
    let mut router = Router::new(RoutingStrategy::Balanced);
    let mut working_memory = WorkingMemory::new(10);

    let episodic_db = prometheus_core::test_support::temp_db_path("e2e-test-episodic-pipeline");
    let _ = std::fs::remove_file(&episodic_db); // 清理旧数据
    let mut episodic_memory = EpisodicMemory::new(episodic_db, 128).unwrap();

    let mut reflection_module = ReflectionModule::new();
    let mut economic_cortex = EconomicCortex::new(BudgetConfig::default());
    let mut predict_module = PredictModule::new();
    let mut belief_module = BeliefModule::new();

    // 2. 模拟用户输入
    let user_input = "帮我写一个 Rust 函数来计算斐波那契数列";

    // 3. 意图分类
    let classification = classifier.classify(user_input).await.unwrap();
    assert!(
        matches!(
            classification.primary,
            IntentType::CodeGen | IntentType::ToolUse
        ),
        "应该识别为代码生成或工具使用意图"
    );
    assert!(classification.confidence > 0.5, "置信度应该大于 0.5");

    // 4. 路由决策
    let route_decision = router.select_model(&classification, 1000);
    assert!(route_decision.quality_score > 0.0, "质量分数应该大于 0");
    assert!(route_decision.estimated_cost >= 0.0, "成本估算应该非负");

    // 5. 工作记忆管理
    working_memory.add(user_input.to_string(), 0.8);
    let context = working_memory.get_active_context(10);
    assert!(!context.is_empty(), "工作记忆应该包含用户输入");
    assert_eq!(context.len(), 1);

    // 6. 情景记忆存储
    let messages = vec![
        Message {
            role: Role::User,
            content: user_input.to_string(),
        },
        Message {
            role: Role::Assistant,
            content: "好的，我来帮你写一个斐波那契函数".to_string(),
        },
    ];
    episodic_memory.add_episode(messages.clone()).await.unwrap();

    // 7. 情景记忆检索
    let search_results = episodic_memory.search("斐波那契", 5).await.unwrap();
    assert!(!search_results.is_empty(), "应该能检索到相关对话");

    // 8. 预测用户行为
    predict_module.record_action(UserAction::RequestCode);
    let predictions = predict_module.predict_next_action();
    assert!(!predictions.is_empty(), "应该能预测下一步行为");

    // 9. 信念更新
    belief_module.update_belief(Evidence::UserFeedback {
        preference: "language".to_string(),
        value: 0.9,
    });
    assert!(belief_module.get_confidence() > 0.0, "信念置信度应该大于 0");

    // 10. 反思对话质量
    let conversation_texts: Vec<String> = messages.iter().map(|m| m.content.clone()).collect();
    let reflection = reflection_module
        .reflect_on_conversation(&conversation_texts, Some(0.85))
        .await;
    assert!(reflection.quality_score > 0.0, "反思质量分数应该大于 0");

    // 11. 成本追踪
    let cost_record = CostRecord {
        timestamp: Utc::now(),
        model_id: format!("{:?}", route_decision.model),
        input_tokens: 50,
        output_tokens: 200,
        cost: 0.05,
        latency_ms: 1200,
        satisfaction: Some(reflection.quality_score),
    };
    economic_cortex.record_cost(cost_record);
    assert!(economic_cortex.get_daily_cost() > 0.0, "日成本应该大于 0");

    // 12. ROI 优化
    let best_model = economic_cortex.get_best_roi_model();
    assert!(best_model.is_some(), "应该能推荐最佳 ROI 模型");
}

// ============================================================================
// Test 2: 多轮对话上下文传递
// ============================================================================

#[tokio::test]
async fn test_multi_turn_conversation_context() {
    let mut working_memory = WorkingMemory::new(5);
    let mut reflection_module = ReflectionModule::new();
    let mut predict_module = PredictModule::new();
    let classifier = IntentClassifier::new();

    let episodic_db = prometheus_core::test_support::temp_db_path("e2e-test-multi-turn");
    let _ = std::fs::remove_file(&episodic_db);
    let mut episodic_memory = EpisodicMemory::new(episodic_db, 128).unwrap();

    // 模拟 6 轮对话
    let turns = [
        ("用户", "你好"),
        ("助手", "你好！有什么可以帮你的？"),
        ("用户", "帮我写代码"),
        ("助手", "好的，需要什么语言？"),
        ("用户", "Rust"),
        ("助手", "明白了，我来写 Rust 代码"),
    ];

    let mut all_messages = Vec::new();

    for (i, (role, text)) in turns.iter().enumerate() {
        let message = Message {
            role: if *role == "用户" {
                Role::User
            } else {
                Role::Assistant
            },
            content: text.to_string(),
        };
        all_messages.push(message);

        if *role == "用户" {
            // 用户消息：添加到工作记忆、意图分类、预测行为
            working_memory.add(text.to_string(), 0.7);
            predict_module.record_action(UserAction::AskQuestion);

            let classification = classifier.classify(text).await.unwrap();
            assert!(
                classification.confidence > 0.0,
                "第 {} 轮：意图分类应该有置信度",
                i + 1
            );
        }
    }

    // 验证工作记忆容量限制
    let context = working_memory.get_active_context(10);
    assert!(context.len() <= 5, "工作记忆应该遵守容量限制");

    // 存储完整对话到情景记忆
    episodic_memory
        .add_episode(all_messages.clone())
        .await
        .unwrap();

    // 检索相关对话
    let search_results = episodic_memory.search("Rust 代码", 5).await.unwrap();
    assert!(!search_results.is_empty(), "应该能检索到 Rust 相关对话");

    // 反思整个对话
    let conversation_texts: Vec<String> = all_messages.iter().map(|m| m.content.clone()).collect();
    let reflection = reflection_module
        .reflect_on_conversation(&conversation_texts, Some(0.8))
        .await;
    assert!(reflection.quality_score > 0.5, "多轮对话质量应该较高");

    // 预测下一步行为
    let predictions = predict_module.predict_next_action();
    assert!(!predictions.is_empty(), "应该能预测用户下一步行为");
}

// ============================================================================
// Test 3: LLM 模块集成（带 Ollama Provider）
// ============================================================================

#[tokio::test]
#[ignore] // 需要真实 Ollama 实例
async fn test_llm_integration_with_ollama() {
    // 初始化 Ollama Provider
    let ollama_provider = Arc::new(OllamaProvider::new()) as Arc<dyn Provider>;

    // 验证 Ollama 可用
    let is_valid = ollama_provider.validate_key().await.unwrap();
    assert!(is_valid, "Ollama 服务应该可用");

    // 1. 意图分类（使用 LLM）
    let classifier = IntentClassifier::new().with_llm_provider(ollama_provider.clone());
    let classification = classifier.classify("帮我写一个排序算法").await.unwrap();
    assert!(
        matches!(classification.primary, IntentType::CodeGen),
        "应该识别为代码生成"
    );
    assert_eq!(classification.confidence, 0.85, "LLM 分类置信度应该是 0.85");

    // 2. 反思模块（使用 LLM）
    let mut reflection_module = ReflectionModule::new().with_llm_provider(ollama_provider.clone());
    let messages = vec![
        "用户：帮我写排序算法".to_string(),
        "助手：好的，我来写一个快速排序".to_string(),
        "用户：能解释一下原理吗？".to_string(),
        "助手：快速排序使用分治法...".to_string(),
    ];
    let reflection = reflection_module
        .reflect_on_conversation(&messages, Some(0.9))
        .await;
    assert!(reflection.quality_score > 0.0, "LLM 反思应该返回质量分数");

    // 3. Embedding 服务（使用 Ollama）
    let embedding_service = Arc::new(ProviderEmbeddingService::new(ollama_provider.clone(), 384));
    let vector = embedding_service.embed("测试文本").await.unwrap();
    assert_eq!(vector.len(), 384, "Embedding 维度应该是 384");
    assert!(vector.iter().any(|&v| v != 0.0), "向量不应该全为 0");
}

// ============================================================================
// Test 4: 经济层完整流程
// ============================================================================

#[tokio::test]
async fn test_economic_cortex_full_workflow() {
    let mut economic_cortex = EconomicCortex::new(BudgetConfig {
        daily_budget: 10.0,
        monthly_budget: 300.0,
        alert_threshold: 0.8,
    });

    let mut reflection_module = ReflectionModule::new();

    // 模拟 5 次对话，记录成本和质量
    let models = vec![
        "gpt-4",
        "gpt-3.5-turbo",
        "claude-3-opus",
        "claude-3-sonnet",
        "qwen2.5:1.5b",
    ];
    let costs = [0.06, 0.002, 0.05, 0.01, 0.0];
    let latencies = [1500, 800, 1200, 900, 3000];
    let satisfactions = [0.95, 0.75, 0.92, 0.85, 0.70];

    for (i, model) in models.iter().enumerate() {
        let messages = vec![format!("用户问题 {}", i + 1), format!("助手回答 {}", i + 1)];
        let reflection = reflection_module
            .reflect_on_conversation(&messages, Some(satisfactions[i]))
            .await;

        let cost_record = CostRecord {
            timestamp: Utc::now(),
            model_id: model.to_string(),
            input_tokens: 100,
            output_tokens: 200,
            cost: costs[i],
            latency_ms: latencies[i],
            satisfaction: Some(reflection.quality_score),
        };
        economic_cortex.record_cost(cost_record);
    }

    // 验证成本追踪
    let daily_cost = economic_cortex.get_daily_cost();
    assert!(daily_cost > 0.0, "日成本应该大于 0");
    assert!(daily_cost < 10.0, "日成本应该在预算内");

    // 验证 ROI 计算
    let best_model = economic_cortex.get_best_roi_model();
    assert!(best_model.is_some(), "应该能推荐最佳 ROI 模型");

    // 验证模型统计
    for model in &models {
        if let Some(s) = economic_cortex.get_model_stats(model) {
            assert!(s.roi_score >= 0.0, "{} 的 ROI 应该非负", model);
        }
    }

    // 验证预算告警（通过检查日成本是否超过阈值）
    let daily_cost = economic_cortex.get_daily_cost();
    let budget = 10.0;
    let threshold = 0.8;
    let should_alert = daily_cost > budget * threshold;
    assert!(!should_alert, "成本未超过告警阈值");
}

// ============================================================================
// Test 5: 错误处理与优雅降级
// ============================================================================

#[tokio::test]
async fn test_graceful_degradation() {
    // 1. 意图分类降级（无 LLM 时使用规则）
    let classifier = IntentClassifier::new(); // 无 LLM provider
    let classification = classifier.classify("写一个函数").await.unwrap();
    assert!(
        matches!(classification.primary, IntentType::CodeGen),
        "规则分类应该正常工作"
    );

    // 2. 反思模块降级（无 LLM 时使用规则）
    let mut reflection_module = ReflectionModule::new(); // 无 LLM provider
    let messages = vec!["用户问题".to_string(), "助手回答".to_string()];
    let reflection = reflection_module
        .reflect_on_conversation(&messages, Some(0.8))
        .await;
    assert!(reflection.quality_score > 0.0, "规则反思应该正常工作");

    // 3. Embedding 降级（使用 Mock）
    let mock_embedding = Arc::new(MockEmbeddingService::new(128)) as Arc<dyn EmbeddingService>;
    let vector = mock_embedding.embed("测试").await.unwrap();
    assert_eq!(vector.len(), 128, "Mock embedding 应该返回正确维度");

    // 4. 路由决策（无实时数据时使用静态能力矩阵）
    let mut router = Router::new(RoutingStrategy::Balanced);
    let classifier = IntentClassifier::new();
    let classification = classifier.classify("简单问题").await.unwrap();
    let decision = router.select_model(&classification, 500);
    assert!(decision.quality_score > 0.0, "静态路由应该正常工作");
}

// ============================================================================
// Test 6: 能力矩阵动态更新
// ============================================================================

#[tokio::test]
async fn test_capability_matrix_dynamic_update() {
    let mut matrix = CapabilityMatrix::new();

    // 初始能力分数
    let initial_score = matrix.get_capability(ModelId::GPT4o, &IntentType::CodeGen);
    assert!(initial_score > 0.0, "初始分数应该大于 0");

    // 模拟多次推理，更新能力分数
    for _ in 0..10 {
        matrix.update_capability(ModelId::GPT4o, IntentType::CodeGen, 0.95);
    }

    let updated_score = matrix.get_capability(ModelId::GPT4o, &IntentType::CodeGen);
    assert!(updated_score >= initial_score, "能力分数应该提升或保持");

    // 测试批量更新
    let updates = vec![
        (ModelId::Claude35Sonnet, IntentType::Reasoning, 0.9),
        (ModelId::Claude35Sonnet, IntentType::Knowledge, 0.85),
    ];
    matrix.update_capabilities(updates);

    let reasoning_score = matrix.get_capability(ModelId::Claude35Sonnet, &IntentType::Reasoning);
    assert!(reasoning_score > 0.0, "批量更新应该生效");
}

// ============================================================================
// Test 7: 完整对话流程（带 Ollama）
// ============================================================================

#[tokio::test]
#[ignore] // 需要真实 Ollama 实例
async fn test_full_conversation_with_ollama() {
    // 初始化 Ollama Provider
    let ollama_provider = Arc::new(OllamaProvider::new()) as Arc<dyn Provider>;

    // 验证 Ollama 可用
    assert!(
        ollama_provider.validate_key().await.unwrap(),
        "Ollama 应该可用"
    );

    // 初始化所有模块（使用 Ollama）
    let classifier = IntentClassifier::new().with_llm_provider(ollama_provider.clone());
    let mut router = Router::new(RoutingStrategy::CostOptimized);
    let mut working_memory = WorkingMemory::new(10);

    let episodic_db = prometheus_core::test_support::temp_db_path("e2e-test-ollama-full");
    let _ = std::fs::remove_file(&episodic_db);
    let embedding_service = Arc::new(ProviderEmbeddingService::new(ollama_provider.clone(), 384));
    let mut episodic_memory =
        EpisodicMemory::with_embedding_service(episodic_db, embedding_service).unwrap();

    let mut reflection_module = ReflectionModule::new().with_llm_provider(ollama_provider.clone());
    let mut economic_cortex = EconomicCortex::default();

    // 用户输入
    let user_input = "帮我写一个 Rust 函数计算阶乘";

    // 1. 意图分类（LLM）
    let classification = classifier.classify(user_input).await.unwrap();
    assert!(matches!(classification.primary, IntentType::CodeGen));

    // 2. 路由决策
    let _route_decision = router.select_model(&classification, 1000);

    // 3. LLM 推理（模拟）
    let chat_request = prometheus_core::providers::traits::ChatRequest {
        model: "qwen2.5:1.5b".to_string(),
        messages: vec![prometheus_core::providers::traits::ChatMessage {
            role: "user".to_string(),
            content: user_input.to_string(),
        }],
        max_tokens: Some(500),
        temperature: Some(0.7),
        stream: false,
    };
    let chat_response = ollama_provider.chat(&chat_request).await.unwrap();
    assert!(!chat_response.content.is_empty(), "LLM 应该返回内容");

    // 4. 工作记忆
    working_memory.add(user_input.to_string(), 0.8);
    working_memory.add(chat_response.content.clone(), 0.9);

    // 5. 情景记忆
    let messages = vec![
        Message {
            role: Role::User,
            content: user_input.to_string(),
        },
        Message {
            role: Role::Assistant,
            content: chat_response.content.clone(),
        },
    ];
    episodic_memory.add_episode(messages.clone()).await.unwrap();

    // 6. 反思（LLM）
    let conversation_texts: Vec<String> = messages.iter().map(|m| m.content.clone()).collect();
    let reflection = reflection_module
        .reflect_on_conversation(&conversation_texts, Some(0.85))
        .await;
    assert!(reflection.quality_score > 0.0);

    // 7. 成本追踪
    let cost_record = CostRecord {
        timestamp: Utc::now(),
        model_id: "qwen2.5:1.5b".to_string(),
        input_tokens: chat_response.usage.prompt_tokens,
        output_tokens: chat_response.usage.completion_tokens,
        cost: 0.0, // 本地模型成本为 0
        latency_ms: 3000,
        satisfaction: Some(reflection.quality_score),
    };
    economic_cortex.record_cost(cost_record);

    // 8. 验证完整流程
    assert!(
        economic_cortex.get_daily_cost() == 0.0,
        "本地模型成本应该为 0"
    );
    let context = working_memory.get_active_context(10);
    assert_eq!(context.len(), 2, "工作记忆应该包含 2 条消息");
}
