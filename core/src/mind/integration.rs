//! 认知集成层
//!
//! 将 PBRIC 循环、好奇心引擎、思维日志、自我进化、经济皮层统一封装，
//! 提供 `process_turn()` 供 ChatOrchestrator 调用。

use super::curiosity::CuriosityEngine;
use super::economics::{BudgetConfig, CostRecord, EconomicCortex};
use super::evolution::{Dimension, EvolutionEngine};
use super::pbric::PbricLoop;
use super::thought_log::{ThinkingPhase, ThoughtLogger};
use chrono::Utc;

/// 认知处理结果
#[derive(Debug, Clone)]
pub struct MindTurnResult {
    /// PBRIC 循环输出
    pub pbric_output: String,
    /// 检测到的知识缺口主题（如有）
    pub knowledge_gaps: Vec<String>,
    /// 本轮置信度
    pub confidence: f64,
    /// 经济皮层是否有告警
    pub budget_alerts: Vec<String>,
    /// 进化维度评分
    pub dimension_scores: Vec<(String, f64)>,
}

/// 认知集成层：统一管理所有 mind 子系统
pub struct MindIntegration {
    pbric: PbricLoop,
    curiosity: CuriosityEngine,
    thought_logger: ThoughtLogger,
    evolution: EvolutionEngine,
    economics: EconomicCortex,
    session_id: String,
    turn_count: u64,
}

impl MindIntegration {
    pub fn new(session_id: &str) -> Self {
        Self {
            pbric: PbricLoop::new().with_session(session_id),
            curiosity: CuriosityEngine::new(),
            thought_logger: ThoughtLogger::new(10000),
            evolution: EvolutionEngine::new(),
            economics: EconomicCortex::new(BudgetConfig::default()),
            session_id: session_id.to_string(),
            turn_count: 0,
        }
    }

    pub fn with_budget(mut self, budget: BudgetConfig) -> Self {
        self.economics = EconomicCortex::new(budget);
        self
    }

    /// 核心方法：处理一轮用户输入
    ///
    /// 流程：
    /// 1. PBRIC 认知循环
    /// 2. 好奇心检测
    /// 3. 经济皮层记账
    /// 4. 进化追踪
    /// 5. 思维日志记录
    pub async fn process_turn(
        &mut self,
        user_input: &str,
        model_id: &str,
        cost: f64,
        latency_ms: u64,
        satisfaction: Option<f64>,
    ) -> anyhow::Result<MindTurnResult> {
        self.turn_count += 1;

        // 1. PBRIC 认知循环
        let pbric_output = self.pbric.run_cycle(user_input).await?;

        // 提取 PBRIC 阶段置信度
        let phase_outputs = self.pbric.phase_outputs();
        let avg_confidence = if phase_outputs.is_empty() {
            0.5
        } else {
            phase_outputs.iter().map(|o| o.confidence).sum::<f64>() / phase_outputs.len() as f64
        };

        // 2. 好奇心检测
        let mut knowledge_gaps = Vec::new();
        // 低置信度 → 可能存在知识缺口
        if avg_confidence < 0.5 {
            let gap = self.curiosity.detect_reasoning_failure(
                user_input,
                &format!("低置信度推理: {:.2}", avg_confidence),
            );
            knowledge_gaps.push(gap.topic);
        }
        // 新领域检测
        let topic = extract_topic(user_input);
        if let Some(gap) = self.curiosity.detect_new_domain(&topic) {
            knowledge_gaps.push(gap.topic);
        }

        // 3. 经济皮层记账
        self.economics.record_cost(CostRecord {
            timestamp: Utc::now(),
            model_id: model_id.to_string(),
            input_tokens: estimate_tokens(user_input),
            output_tokens: estimate_tokens(&pbric_output),
            cost,
            latency_ms,
            satisfaction,
        });
        let budget_alerts = self.economics.check_budget_alert();

        // 4. 进化追踪：根据置信度评估各维度
        let dimension = classify_dimension(user_input);
        let score = avg_confidence * 100.0;
        self.evolution
            .record_event(dimension.clone(), score, user_input);

        let dimension_scores: Vec<(String, f64)> = [
            Dimension::Conversation,
            Dimension::Reasoning,
            Dimension::Creativity,
            Dimension::ToolUse,
        ]
        .iter()
        .map(|d| (format!("{:?}", d), self.evolution.get_score(d)))
        .collect();

        // 5. 思维日志：记录本轮总结
        self.thought_logger.log_thought(
            &self.session_id,
            ThinkingPhase::Reflect,
            &format!(
                "第 {} 轮完成，置信度 {:.2}",
                self.turn_count, avg_confidence
            ),
            &format!(
                "模型={}, 成本=${:.4}, 延迟={}ms, 缺口={}",
                model_id,
                cost,
                latency_ms,
                knowledge_gaps.len()
            ),
            user_input,
            Some(&pbric_output),
            avg_confidence,
            latency_ms,
        );

        Ok(MindTurnResult {
            pbric_output,
            knowledge_gaps,
            confidence: avg_confidence,
            budget_alerts,
            dimension_scores,
        })
    }

    /// 获取好奇心引擎引用
    pub fn curiosity(&self) -> &CuriosityEngine {
        &self.curiosity
    }

    /// 获取好奇心引擎可变引用
    pub fn curiosity_mut(&mut self) -> &mut CuriosityEngine {
        &mut self.curiosity
    }

    /// 获取经济皮层引用
    pub fn economics(&self) -> &EconomicCortex {
        &self.economics
    }

    /// 获取进化引擎引用
    pub fn evolution(&self) -> &EvolutionEngine {
        &self.evolution
    }

    /// 获取思维日志引用
    pub fn thought_logger(&self) -> &ThoughtLogger {
        &self.thought_logger
    }

    /// 获取 PBRIC 引用
    pub fn pbric(&self) -> &PbricLoop {
        &self.pbric
    }

    /// 获取轮次计数
    pub fn turn_count(&self) -> u64 {
        self.turn_count
    }
}

/// 从用户输入中提取主题（简化：取前 20 字符）
fn extract_topic(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() <= 20 {
        trimmed.to_string()
    } else {
        trimmed.chars().take(20).collect()
    }
}

/// 估算 token 数（简化：中文按字符，英文按空格分词）
fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    let words = text.split_whitespace().count();
    // 中文为主时按字符数，英文为主时按词数
    if chars > words * 2 {
        chars
    } else {
        words
    }
}

/// 根据输入内容分类能力维度
fn classify_dimension(input: &str) -> Dimension {
    let lower = input.to_lowercase();
    if lower.contains("代码")
        || lower.contains("code")
        || lower.contains("bug")
        || lower.contains("编程")
        || lower.contains("函数")
    {
        Dimension::ToolUse
    } else if lower.contains("推理")
        || lower.contains("分析")
        || lower.contains("为什么")
        || lower.contains("原因")
        || lower.contains("逻辑")
    {
        Dimension::Reasoning
    } else if lower.contains("写")
        || lower.contains("诗")
        || lower.contains("故事")
        || lower.contains("创作")
        || lower.contains("设计")
    {
        Dimension::Creativity
    } else {
        Dimension::Conversation
    }
}

impl Default for MindIntegration {
    fn default() -> Self {
        Self::new("default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_turn_basic() {
        let mut mind = MindIntegration::new("test_session");
        let result = mind
            .process_turn("你好", "local-7b", 0.0, 100, Some(0.8))
            .await
            .unwrap();

        assert!(!result.pbric_output.is_empty());
        assert!(result.confidence > 0.0);
        assert_eq!(mind.turn_count(), 1);
    }

    #[tokio::test]
    async fn test_process_turn_records_economics() {
        let mut mind = MindIntegration::new("econ_test");
        mind.process_turn("测试", "gpt-4", 0.05, 1000, Some(0.9))
            .await
            .unwrap();

        let stats = mind.economics().get_model_stats("gpt-4");
        assert!(stats.is_some());
        assert_eq!(stats.unwrap().call_count, 1);
    }

    #[tokio::test]
    async fn test_process_turn_tracks_evolution() {
        let mut mind = MindIntegration::new("evo_test");
        mind.process_turn("你好啊", "local", 0.0, 50, Some(0.8))
            .await
            .unwrap();

        let result = mind
            .process_turn("帮我写代码", "local", 0.0, 50, Some(0.7))
            .await
            .unwrap();

        // 应该有 4 个维度的分数
        assert_eq!(result.dimension_scores.len(), 4);
        // 进化引擎应有 2 个事件
        assert_eq!(mind.evolution().events().len(), 2);
    }

    #[tokio::test]
    async fn test_process_turn_detects_new_domain() {
        let mut mind = MindIntegration::new("curiosity_test");
        let result = mind
            .process_turn("量子计算是什么", "local", 0.0, 50, Some(0.8))
            .await
            .unwrap();

        // 新领域应被检测
        assert!(!result.knowledge_gaps.is_empty() || mind.curiosity().gaps().len() > 0);
    }

    #[tokio::test]
    async fn test_process_turn_budget_alert() {
        let budget = BudgetConfig {
            daily_budget: 0.01,
            monthly_budget: 0.3,
            alert_threshold: 0.8,
        };
        let mut mind = MindIntegration::new("budget_test").with_budget(budget);
        let result = mind
            .process_turn("测试", "gpt-4", 0.02, 1000, Some(0.9))
            .await
            .unwrap();

        // 超预算应有告警
        assert!(!result.budget_alerts.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_turns_accumulate() {
        let mut mind = MindIntegration::new("multi_test");

        for i in 0..5 {
            mind.process_turn(&format!("第 {} 轮对话", i), "local", 0.0, 50, Some(0.8))
                .await
                .unwrap();
        }

        assert_eq!(mind.turn_count(), 5);
        // 思维日志应有记录（PBRIC 每轮 5 条 + reflect 1 条 = 6 条/轮）
        assert!(mind.thought_logger().len() > 0);
        // PBRIC 内部思维链应有 5 条
        assert_eq!(mind.pbric().thought_logger().chain_count(), 5);
    }

    #[test]
    fn test_extract_topic() {
        assert_eq!(extract_topic("短文本"), "短文本");
        let long = "这是一段非常长的文本用来测试主题提取功能是否正确截断";
        assert_eq!(extract_topic(long).chars().count(), 20);
    }

    #[test]
    fn test_estimate_tokens() {
        assert!(estimate_tokens("hello world") > 0);
        assert!(estimate_tokens("你好世界") > 0);
    }

    #[test]
    fn test_classify_dimension() {
        assert_eq!(classify_dimension("帮我写代码"), Dimension::ToolUse);
        assert_eq!(classify_dimension("为什么会这样"), Dimension::Reasoning);
        assert_eq!(classify_dimension("写一首诗"), Dimension::Creativity);
        assert_eq!(classify_dimension("你好"), Dimension::Conversation);
    }
}
