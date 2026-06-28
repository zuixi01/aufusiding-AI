//! P3 智能进化 — 跨模块集成测试

use prometheus_core::mind::curiosity::{CuriosityEngine, GapSource};
use prometheus_core::mind::thought_log::{ThinkingPhase, ThoughtLogger, ThoughtQuery};
use prometheus_core::mind::evolution::{Dimension, EvolutionEngine};
use prometheus_core::mind::economics::{BudgetConfig, EconomicCortex, CostRecord, RecommendationPriority};
use prometheus_core::mind::pbric::PbricLoop;
use chrono::Utc;

// ── 场景 1: PBRIC 完整循环 + 思维日志记录 ──

#[tokio::test]
async fn pbric_cycle_logs_all_phases() {
    let mut pbric = PbricLoop::new().with_session("integration_s1");
    let result = pbric.run_cycle("什么是量子计算？").await.unwrap();

    assert!(result.contains("commit"));
    assert!(result.contains("action:"));

    // 思维日志应记录 5 个阶段
    let logger = pbric.thought_logger();
    assert!(logger.len() >= 5);
    assert_eq!(logger.chain_count(), 1);

    // 信念应该被更新
    let state = pbric.belief().get_state();
    assert!(!state.preferences.is_empty());
}

// ── 场景 2: 好奇心检测 + 探索计划生成 ──

#[test]
fn curiosity_detects_gaps_and_creates_plans() {
    let mut engine = CuriosityEngine::new();
    engine.register_known_topic("rust");

    // 新领域触发缺口
    let gap = engine.detect_new_domain("量子力学").unwrap();
    assert_eq!(gap.source, GapSource::NewDomain);

    // 重复提问提升重要性
    let g1 = engine.detect_unanswered("暗物质");
    let g2 = engine.detect_unanswered("暗物质");
    assert!(g2.importance > g1.importance);

    // 为高优先级缺口生成探索计划
    let conflict_gap = engine.detect_belief_conflict("宇宙年龄");
    let plan = engine.create_exploration_plan(&conflict_gap);
    assert!(!plan.steps.is_empty());
    assert!(plan.expected_value > 0.0);

    let stats = engine.stats();
    assert_eq!(stats.total_gaps, 4);
    assert_eq!(stats.active_plans, 1);
}

// ── 场景 3: 思维日志查询和摘要 ──

#[test]
fn thought_log_query_and_summarize() {
    let mut logger = ThoughtLogger::new(1000);
    let chain_id = logger.begin_chain("s1", "测试输入");

    logger.log_thought("s1", ThinkingPhase::Perceive, "感知", "接收输入", "ctx", None, 0.9, 10);
    logger.log_thought("s1", ThinkingPhase::Believe, "信念", "更新偏好", "ctx", None, 0.7, 20);
    logger.log_thought("s1", ThinkingPhase::Reason, "推理", "分析意图", "ctx", Some("直接回答"), 0.8, 30);
    logger.log_thought("s1", ThinkingPhase::Intend, "意图", "形成计划", "ctx", Some("回答"), 0.85, 15);
    logger.log_thought("s1", ThinkingPhase::Commit, "承诺", "执行输出", "ctx", Some("result"), 0.95, 5);

    logger.end_chain(Some("最终输出"));

    // 查询特定阶段
    let query = ThoughtQuery {
        phase: Some(ThinkingPhase::Reason),
        ..Default::default()
    };
    let results = logger.query(&query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].summary, "推理");

    // 获取思维链
    let chain = logger.get_chain(&chain_id).unwrap();
    assert_eq!(chain.entries.len(), 5);
    assert_eq!(chain.final_output.as_deref(), Some("最终输出"));

    // 摘要
    let summary = logger.summarize_recent(10);
    assert_eq!(summary.entry_count, 5);
    assert!(summary.avg_confidence > 0.7);
}

// ── 场景 4: 自我进化四维追踪 + 成长报告 ──

#[test]
fn evolution_tracks_growth_and_generates_report() {
    let mut engine = EvolutionEngine::new();

    // 模拟对话能力持续提升
    for i in 0..10 {
        engine.record_event(Dimension::Conversation, 50.0 + i as f64 * 5.0, "chat");
    }
    // 推理能力高水平
    for _ in 0..10 {
        engine.record_event(Dimension::Reasoning, 85.0, "logic");
    }
    // 创意能力较低
    for _ in 0..10 {
        engine.record_event(Dimension::Creativity, 30.0, "creative");
    }
    // 工具能力中等
    for _ in 0..5 {
        engine.record_event(Dimension::ToolUse, 60.0, "tool");
    }

    let report = engine.generate_report();
    assert_eq!(report.dimensions.len(), 4);
    assert!(report.total_evaluations >= 35);

    // 推理应该是亮点
    assert!(report.highlights.iter().any(|h| h.contains("Reasoning")));
    // 创意应该有建议
    assert!(report.suggestions.iter().any(|s| s.contains("Creativity")));

    // 对话趋势应为正
    let conv = report.dimensions.get("Conversation").unwrap();
    assert!(conv.trend > 0.0);

    // 最强/最弱维度
    let (strongest, _) = engine.strongest_dimension().unwrap();
    assert_eq!(strongest, Dimension::Reasoning);
    let (weakest, _) = engine.weakest_dimension().unwrap();
    assert_eq!(weakest, Dimension::Creativity);
}

// ── 场景 5: 经济皮层 v2 仪表盘 + 经济代理 ──

#[test]
fn economic_cortex_v2_dashboard_and_agent() {
    let budget = BudgetConfig {
        daily_budget: 0.10,
        monthly_budget: 3.0,
        alert_threshold: 0.8,
    };
    let mut cortex = EconomicCortex::new(budget);

    cortex.record_cost(CostRecord {
        timestamp: Utc::now(),
        model_id: "local-7b".to_string(),
        input_tokens: 200, output_tokens: 100,
        cost: 0.0, latency_ms: 100,
        satisfaction: Some(0.75),
    });
    cortex.record_cost(CostRecord {
        timestamp: Utc::now(),
        model_id: "cloud-gpt4".to_string(),
        input_tokens: 500, output_tokens: 300,
        cost: 0.12, latency_ms: 1200,
        satisfaction: Some(0.9),
    });

    // 仪表盘
    let dash = cortex.dashboard();
    assert_eq!(dash.total_calls, 2);
    assert_eq!(dash.model_count, 2);
    assert!(dash.avg_satisfaction > 0.7);
    assert!(dash.daily_utilization > 1.0); // 超预算

    // 经济代理应发出超支告警
    let recs = cortex.agent_recommendations();
    assert!(!recs.is_empty());
    assert!(recs.iter().any(|r| r.priority == RecommendationPriority::Critical));
}

// ── 场景 6: 多轮 PBRIC + 进化追踪协同 ──

#[tokio::test]
async fn pbric_with_evolution_tracking() {
    let mut pbric = PbricLoop::new().with_session("evo_test");
    let mut evolution = EvolutionEngine::new();

    // 模拟多轮对话
    let inputs = vec!["你好", "什么是 Rust？", "帮我写一首诗", "这个 bug 怎么修？"];
    for input in &inputs {
        let result = pbric.run_cycle(input).await.unwrap();
        assert!(result.contains("commit"));

        // 根据输出评估对话能力
        let outputs = pbric.phase_outputs();
        let avg_conf: f64 = outputs.iter().map(|o| o.confidence).sum::<f64>() / outputs.len() as f64;
        evolution.record_event(Dimension::Conversation, avg_conf * 100.0, input);
    }

    // 进化引擎应有记录
    assert_eq!(evolution.events().len(), 4);
    let score = evolution.get_score(&Dimension::Conversation);
    assert!(score > 0.0);

    // 思维日志应有 4 条链
    let logger = pbric.thought_logger();
    assert_eq!(logger.chain_count(), 4);
}
