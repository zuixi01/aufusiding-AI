//! 好奇心驱动系统
//!
//! 检测知识缺口，触发主动学习，规划知识探索路径。
//! 依赖 BeliefState 的偏好和目标信息，以及 PredictModule 的行为模式。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 知识缺口
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGap {
    /// 缺口主题
    pub topic: String,
    /// 检测来源（用户提问、推理失败、信念冲突等）
    pub source: GapSource,
    /// 重要性 [0.0, 1.0]
    pub importance: f64,
    /// 检测时间
    pub detected_at: DateTime<Utc>,
    /// 是否已探索
    pub explored: bool,
}

/// 缺口来源
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GapSource {
    /// 用户提问但无法回答
    UnansweredQuery,
    /// 推理过程中发现缺失
    ReasoningFailure,
    /// 信念冲突需要更多信息
    BeliefConflict,
    /// 预测准确率下降
    PredictionDrift,
    /// 新领域接触
    NewDomain,
}

/// 探索计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationPlan {
    /// 计划 ID
    pub id: String,
    /// 目标主题
    pub topic: String,
    /// 探索步骤
    pub steps: Vec<ExplorationStep>,
    /// 预期收益 [0.0, 1.0]
    pub expected_value: f64,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

/// 探索步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationStep {
    /// 步骤描述
    pub description: String,
    /// 步骤类型
    pub step_type: StepType,
    /// 是否完成
    pub completed: bool,
}

/// 步骤类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepType {
    /// 搜索已有知识
    SearchExisting,
    /// 向用户提问
    AskUser,
    /// 推理推导
    Reason,
    /// 实验验证
    Experiment,
}

/// 好奇心驱动引擎
pub struct CuriosityEngine {
    /// 已检测的知识缺口
    gaps: Vec<KnowledgeGap>,
    /// 已知主题集合（用于检测新领域）
    known_topics: HashSet<String>,
    /// 探索计划队列
    plans: Vec<ExplorationPlan>,
    /// 好奇心强度 [0.0, 1.0]，影响主动探索频率
    curiosity_level: f64,
    /// 主题查询失败计数
    failure_counts: HashMap<String, usize>,
    /// 最小重要性阈值（低于此值的缺口不触发探索）
    min_importance: f64,
    /// 下一个计划 ID 计数器
    next_plan_id: u64,
}

impl CuriosityEngine {
    /// 创建好奇心引擎
    pub fn new() -> Self {
        Self {
            gaps: Vec::new(),
            known_topics: HashSet::new(),
            plans: Vec::new(),
            curiosity_level: 0.5,
            failure_counts: HashMap::new(),
            min_importance: 0.3,
            next_plan_id: 1,
        }
    }

    /// 注册已知主题
    pub fn register_known_topic(&mut self, topic: &str) {
        self.known_topics.insert(topic.to_lowercase());
    }

    /// 检测知识缺口：用户提问无法回答
    pub fn detect_unanswered(&mut self, topic: &str) -> KnowledgeGap {
        let count = self.failure_counts.entry(topic.to_lowercase()).or_insert(0);
        *count += 1;
        let importance = (0.3 + 0.15 * (*count as f64)).min(1.0);

        let gap = KnowledgeGap {
            topic: topic.to_string(),
            source: GapSource::UnansweredQuery,
            importance,
            detected_at: Utc::now(),
            explored: false,
        };
        self.gaps.push(gap.clone());
        gap
    }

    /// 检测知识缺口：推理失败
    pub fn detect_reasoning_failure(&mut self, topic: &str, context: &str) -> KnowledgeGap {
        let importance = if context.len() > 100 { 0.7 } else { 0.5 };
        let gap = KnowledgeGap {
            topic: topic.to_string(),
            source: GapSource::ReasoningFailure,
            importance,
            detected_at: Utc::now(),
            explored: false,
        };
        self.gaps.push(gap.clone());
        gap
    }

    /// 检测知识缺口：新领域
    pub fn detect_new_domain(&mut self, topic: &str) -> Option<KnowledgeGap> {
        let normalized = topic.to_lowercase();
        if self.known_topics.contains(&normalized) {
            return None;
        }
        self.known_topics.insert(normalized);
        let gap = KnowledgeGap {
            topic: topic.to_string(),
            source: GapSource::NewDomain,
            importance: 0.6,
            detected_at: Utc::now(),
            explored: false,
        };
        self.gaps.push(gap.clone());
        Some(gap)
    }

    /// 检测知识缺口：信念冲突
    pub fn detect_belief_conflict(&mut self, topic: &str) -> KnowledgeGap {
        let gap = KnowledgeGap {
            topic: topic.to_string(),
            source: GapSource::BeliefConflict,
            importance: 0.8,
            detected_at: Utc::now(),
            explored: false,
        };
        self.gaps.push(gap.clone());
        gap
    }

    /// 获取未探索的高优先级缺口（按重要性降序）
    pub fn get_pending_gaps(&self) -> Vec<&KnowledgeGap> {
        let mut pending: Vec<&KnowledgeGap> = self
            .gaps
            .iter()
            .filter(|g| !g.explored && g.importance >= self.min_importance)
            .collect();
        pending.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        pending
    }

    /// 为知识缺口生成探索计划
    pub fn create_exploration_plan(&mut self, gap: &KnowledgeGap) -> ExplorationPlan {
        let steps = match &gap.source {
            GapSource::UnansweredQuery => vec![
                ExplorationStep {
                    description: format!("搜索已有知识库中关于「{}」的信息", gap.topic),
                    step_type: StepType::SearchExisting,
                    completed: false,
                },
                ExplorationStep {
                    description: format!("向用户确认「{}」的具体需求", gap.topic),
                    step_type: StepType::AskUser,
                    completed: false,
                },
            ],
            GapSource::ReasoningFailure => vec![
                ExplorationStep {
                    description: format!("分析「{}」推理失败的原因", gap.topic),
                    step_type: StepType::Reason,
                    completed: false,
                },
                ExplorationStep {
                    description: format!("搜索「{}」相关的补充知识", gap.topic),
                    step_type: StepType::SearchExisting,
                    completed: false,
                },
            ],
            GapSource::BeliefConflict => vec![
                ExplorationStep {
                    description: format!("收集「{}」的多方证据", gap.topic),
                    step_type: StepType::SearchExisting,
                    completed: false,
                },
                ExplorationStep {
                    description: format!("通过推理验证「{}」的不同观点", gap.topic),
                    step_type: StepType::Reason,
                    completed: false,
                },
                ExplorationStep {
                    description: format!("实验验证「{}」的结论", gap.topic),
                    step_type: StepType::Experiment,
                    completed: false,
                },
            ],
            GapSource::NewDomain => vec![
                ExplorationStep {
                    description: format!("建立「{}」领域的基础概念图", gap.topic),
                    step_type: StepType::SearchExisting,
                    completed: false,
                },
                ExplorationStep {
                    description: format!("向用户了解「{}」的使用场景", gap.topic),
                    step_type: StepType::AskUser,
                    completed: false,
                },
            ],
            GapSource::PredictionDrift => vec![
                ExplorationStep {
                    description: format!("分析「{}」预测偏差的模式", gap.topic),
                    step_type: StepType::Reason,
                    completed: false,
                },
                ExplorationStep {
                    description: format!("搜索「{}」的最新变化", gap.topic),
                    step_type: StepType::SearchExisting,
                    completed: false,
                },
            ],
        };

        let plan_id = format!("plan_{}", self.next_plan_id);
        self.next_plan_id += 1;

        let plan = ExplorationPlan {
            id: plan_id,
            topic: gap.topic.clone(),
            steps,
            expected_value: gap.importance * self.curiosity_level,
            created_at: Utc::now(),
        };
        self.plans.push(plan.clone());
        plan
    }

    /// 标记缺口为已探索
    pub fn mark_explored(&mut self, topic: &str) {
        for gap in &mut self.gaps {
            if gap.topic == topic && !gap.explored {
                gap.explored = true;
            }
        }
    }

    /// 完成探索计划中的某个步骤
    pub fn complete_step(&mut self, plan_id: &str, step_index: usize) -> bool {
        if let Some(plan) = self.plans.iter_mut().find(|p| p.id == plan_id) {
            if let Some(step) = plan.steps.get_mut(step_index) {
                step.completed = true;
                return true;
            }
        }
        false
    }

    /// 获取好奇心强度
    pub fn curiosity_level(&self) -> f64 {
        self.curiosity_level
    }

    /// 根据探索成果调整好奇心强度
    pub fn adjust_curiosity(&mut self, exploration_success_rate: f64) {
        // 成功率高 → 好奇心增强（正反馈）
        // 成功率低 → 好奇心略降（避免浪费资源）
        let delta = (exploration_success_rate - 0.5) * 0.1;
        self.curiosity_level = (self.curiosity_level + delta).clamp(0.1, 1.0);
    }

    /// 获取所有缺口
    pub fn gaps(&self) -> &[KnowledgeGap] {
        &self.gaps
    }

    /// 获取所有探索计划
    pub fn plans(&self) -> &[ExplorationPlan] {
        &self.plans
    }

    /// 统计信息
    pub fn stats(&self) -> CuriosityStats {
        let total_gaps = self.gaps.len();
        let explored = self.gaps.iter().filter(|g| g.explored).count();
        let pending = total_gaps - explored;
        let active_plans = self.plans.len();
        CuriosityStats {
            total_gaps,
            explored_gaps: explored,
            pending_gaps: pending,
            active_plans,
            curiosity_level: self.curiosity_level,
        }
    }
}

/// 好奇心统计
#[derive(Debug, Clone)]
pub struct CuriosityStats {
    pub total_gaps: usize,
    pub explored_gaps: usize,
    pub pending_gaps: usize,
    pub active_plans: usize,
    pub curiosity_level: f64,
}

impl Default for CuriosityEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_engine() {
        let engine = CuriosityEngine::new();
        assert_eq!(engine.curiosity_level(), 0.5);
        assert!(engine.gaps().is_empty());
        assert!(engine.plans().is_empty());
    }

    #[test]
    fn test_detect_unanswered_query() {
        let mut engine = CuriosityEngine::new();
        let gap = engine.detect_unanswered("量子计算");

        assert_eq!(gap.topic, "量子计算");
        assert_eq!(gap.source, GapSource::UnansweredQuery);
        assert!(!gap.explored);
        assert_eq!(engine.gaps().len(), 1);
    }

    #[test]
    fn test_repeated_unanswered_increases_importance() {
        let mut engine = CuriosityEngine::new();
        let gap1 = engine.detect_unanswered("量子计算");
        let gap2 = engine.detect_unanswered("量子计算");
        let gap3 = engine.detect_unanswered("量子计算");

        assert!(gap2.importance > gap1.importance);
        assert!(gap3.importance > gap2.importance);
    }

    #[test]
    fn test_detect_new_domain() {
        let mut engine = CuriosityEngine::new();
        engine.register_known_topic("rust");

        // 已知领域不产生缺口
        assert!(engine.detect_new_domain("Rust").is_none());

        // 新领域产生缺口
        let gap = engine.detect_new_domain("量子力学").unwrap();
        assert_eq!(gap.source, GapSource::NewDomain);

        // 同一领域不重复检测
        assert!(engine.detect_new_domain("量子力学").is_none());
    }

    #[test]
    fn test_detect_belief_conflict() {
        let mut engine = CuriosityEngine::new();
        let gap = engine.detect_belief_conflict("地球年龄");

        assert_eq!(gap.source, GapSource::BeliefConflict);
        assert_eq!(gap.importance, 0.8);
    }

    #[test]
    fn test_detect_reasoning_failure() {
        let mut engine = CuriosityEngine::new();
        let short_ctx = "简短上下文";
        let gap_short = engine.detect_reasoning_failure("数学证明", short_ctx);
        assert_eq!(gap_short.importance, 0.5);

        let long_ctx = "a".repeat(150);
        let gap_long = engine.detect_reasoning_failure("复杂推理", &long_ctx);
        assert_eq!(gap_long.importance, 0.7);
    }

    #[test]
    fn test_get_pending_gaps_sorted() {
        let mut engine = CuriosityEngine::new();
        engine.detect_unanswered("低优先级"); // importance ~0.45
        engine.detect_belief_conflict("高优先级"); // importance 0.8

        let pending = engine.get_pending_gaps();
        assert_eq!(pending.len(), 2);
        assert!(pending[0].importance >= pending[1].importance);
        assert_eq!(pending[0].topic, "高优先级");
    }

    #[test]
    fn test_mark_explored() {
        let mut engine = CuriosityEngine::new();
        engine.detect_unanswered("主题A");
        engine.detect_unanswered("主题B");

        engine.mark_explored("主题A");

        let pending = engine.get_pending_gaps();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].topic, "主题B");
    }

    #[test]
    fn test_create_exploration_plan() {
        let mut engine = CuriosityEngine::new();
        let gap = engine.detect_unanswered("Rust 异步");

        let plan = engine.create_exploration_plan(&gap);
        assert_eq!(plan.topic, "Rust 异步");
        assert!(!plan.steps.is_empty());
        assert!(plan.expected_value > 0.0);
        assert_eq!(engine.plans().len(), 1);
    }

    #[test]
    fn test_exploration_plan_steps_vary_by_source() {
        let mut engine = CuriosityEngine::new();

        let gap_query = KnowledgeGap {
            topic: "t1".into(),
            source: GapSource::UnansweredQuery,
            importance: 0.5,
            detected_at: Utc::now(),
            explored: false,
        };
        let plan_query = engine.create_exploration_plan(&gap_query);

        let gap_conflict = KnowledgeGap {
            topic: "t2".into(),
            source: GapSource::BeliefConflict,
            importance: 0.8,
            detected_at: Utc::now(),
            explored: false,
        };
        let plan_conflict = engine.create_exploration_plan(&gap_conflict);

        // BeliefConflict 应有更多步骤（含实验验证）
        assert!(plan_conflict.steps.len() > plan_query.steps.len());
    }

    #[test]
    fn test_complete_step() {
        let mut engine = CuriosityEngine::new();
        let gap = engine.detect_unanswered("topic");
        let plan = engine.create_exploration_plan(&gap);
        let plan_id = plan.id.clone();

        assert!(engine.complete_step(&plan_id, 0));
        assert!(engine.plans()[0].steps[0].completed);
        assert!(!engine.plans()[0].steps[1].completed);

        // 无效索引
        assert!(!engine.complete_step(&plan_id, 99));
        // 无效 plan_id
        assert!(!engine.complete_step("nonexistent", 0));
    }

    #[test]
    fn test_adjust_curiosity() {
        let mut engine = CuriosityEngine::new();
        let initial = engine.curiosity_level();

        // 高成功率 → 好奇心增强
        engine.adjust_curiosity(0.9);
        assert!(engine.curiosity_level() > initial);

        // 低成功率 → 好奇心降低
        let before = engine.curiosity_level();
        engine.adjust_curiosity(0.1);
        assert!(engine.curiosity_level() < before);
    }

    #[test]
    fn test_curiosity_level_clamped() {
        let mut engine = CuriosityEngine::new();

        // 反复高成功率不超过 1.0
        for _ in 0..100 {
            engine.adjust_curiosity(1.0);
        }
        assert!(engine.curiosity_level() <= 1.0);

        // 反复低成功率不低于 0.1
        for _ in 0..100 {
            engine.adjust_curiosity(0.0);
        }
        assert!(engine.curiosity_level() >= 0.1);
    }

    #[test]
    fn test_stats() {
        let mut engine = CuriosityEngine::new();
        engine.detect_unanswered("A");
        engine.detect_unanswered("B");
        engine.mark_explored("A");

        let gap = engine.detect_belief_conflict("C");
        engine.create_exploration_plan(&gap);

        let stats = engine.stats();
        assert_eq!(stats.total_gaps, 3);
        assert_eq!(stats.explored_gaps, 1);
        assert_eq!(stats.pending_gaps, 2);
        assert_eq!(stats.active_plans, 1);
    }

    #[test]
    fn test_min_importance_filter() {
        let mut engine = CuriosityEngine::new();
        engine.min_importance = 0.6;

        engine.detect_unanswered("低重要性"); // ~0.45
        engine.detect_belief_conflict("高重要性"); // 0.8

        let pending = engine.get_pending_gaps();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].topic, "高重要性");
    }
}
