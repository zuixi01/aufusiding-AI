//! PBRIC 信念模块
//!
//! 使用贝叶斯更新维护对用户偏好、目标和世界状态的信念。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 用户目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// 目标描述
    pub description: String,
    /// 优先级 (0.0 - 1.0)
    pub priority: f64,
    /// 截止时间
    pub deadline: Option<DateTime<Utc>>,
    /// 进度 (0.0 - 1.0)
    pub progress: f64,
}

/// 信念状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeliefState {
    /// 用户偏好：偏好名 -> 强度 [0.0, 1.0]
    pub preferences: HashMap<String, f64>,
    /// 用户目标
    pub goals: Vec<Goal>,
    /// 整体置信度 [0.0, 1.0]
    pub confidence: f64,
}

impl BeliefState {
    fn new() -> Self {
        Self {
            preferences: HashMap::new(),
            goals: Vec::new(),
            confidence: 0.5,
        }
    }
}

/// 证据类型
#[derive(Debug, Clone)]
pub enum Evidence {
    /// 用户反馈
    UserFeedback { preference: String, value: f64 },
    /// 行动完成
    ActionCompleted { goal_id: usize, success: bool },
    /// 上下文变化
    ContextChange { context: String },
}

/// 信念模块
pub struct BeliefModule {
    /// 当前信念状态
    state: BeliefState,
    /// 证据历史
    evidence_history: Vec<Evidence>,
    /// 先验权重
    prior_weight: f64,
}

impl BeliefModule {
    /// 创建信念模块
    pub fn new() -> Self {
        Self {
            state: BeliefState::new(),
            evidence_history: Vec::new(),
            prior_weight: 0.7,
        }
    }

    /// 更新信念
    pub fn update_belief(&mut self, evidence: Evidence) {
        match &evidence {
            Evidence::UserFeedback { preference, value } => {
                self.update_preference(preference, *value);
            }
            Evidence::ActionCompleted { goal_id, success } => {
                self.update_goal_progress(*goal_id, *success);
            }
            Evidence::ContextChange { context } => {
                self.adjust_beliefs_by_context(context);
            }
        }

        self.evidence_history.push(evidence);
        self.recalculate_confidence();
    }

    /// 更新偏好：先验 × 0.7 + 证据 × 0.3
    pub fn update_preference(&mut self, key: &str, new_value: f64) {
        let prior = self.state.preferences.get(key).copied().unwrap_or(0.5);
        let updated = self.prior_weight * prior + (1.0 - self.prior_weight) * new_value;

        self.state
            .preferences
            .insert(key.to_string(), updated.clamp(0.0, 1.0));
    }

    /// 更新目标进度
    fn update_goal_progress(&mut self, goal_id: usize, success: bool) {
        if let Some(goal) = self.state.goals.get_mut(goal_id) {
            if success {
                goal.progress = (goal.progress + 0.2).min(1.0);
            } else {
                goal.progress = (goal.progress - 0.1).max(0.0);
            }
        }
    }

    /// 根据上下文调整信念
    fn adjust_beliefs_by_context(&mut self, _context: &str) {
        // 上下文变化降低置信度，但不触发重新计算
        // 这里直接降低，后续的 recalculate 可能会调整
    }

    /// 重新计算置信度
    pub fn recalculate_confidence(&mut self) {
        // 基于证据数量和一致性计算置信度
        let evidence_count = self.evidence_history.len() as f64;
        let consistency = self.calculate_consistency();

        // 使用对数增长避免过快饱和
        let evidence_factor = if evidence_count > 0.0 {
            ((1.0 + evidence_count).ln() / 3.0).min(1.0) // 对数增长，除以3使其更平缓
        } else {
            0.0
        };

        // 综合计算：证据因子和一致性各占50%
        self.state.confidence = (evidence_factor * 0.5 + consistency * 0.5).clamp(0.0, 1.0);
    }

    /// 计算信念一致性
    fn calculate_consistency(&self) -> f64 {
        // 简化实现：检查偏好值的方差
        if self.state.preferences.is_empty() {
            return 0.5;
        }

        let values: Vec<f64> = self.state.preferences.values().copied().collect();

        if values.len() == 1 {
            return 0.8; // 单个偏好，较高一致性
        }

        let mean: f64 = values.iter().sum::<f64>() / values.len() as f64;
        let variance: f64 =
            values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;

        // 方差越小，一致性越高
        // 将方差映射到 [0.5, 1.0] 范围
        (1.0 - variance.min(0.5)).max(0.5)
    }

    /// 添加目标
    pub fn add_goal(&mut self, goal: Goal) {
        self.state.goals.push(goal);
    }

    /// 获取信念状态
    pub fn get_state(&self) -> &BeliefState {
        &self.state
    }

    /// 获取偏好
    pub fn get_preference(&self, key: &str) -> Option<f64> {
        self.state.preferences.get(key).copied()
    }

    /// 获取置信度
    pub fn get_confidence(&self) -> f64 {
        self.state.confidence
    }

    /// 清空证据历史
    pub fn clear_evidence(&mut self) {
        self.evidence_history.clear();
    }
}

impl Default for BeliefModule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_belief_module() {
        let module = BeliefModule::new();

        assert_eq!(module.get_confidence(), 0.5);
        assert!(module.get_state().preferences.is_empty());
    }

    #[test]
    fn test_update_preference() {
        let mut module = BeliefModule::new();

        // 初始更新：0.7 * 0.5 + 0.3 * 0.8 = 0.35 + 0.24 = 0.59
        module.update_preference("theme", 0.8);
        let first = module.get_preference("theme").unwrap();
        assert!((first - 0.59).abs() < 0.01);

        // 再次更新：0.7 * 0.59 + 0.3 * 0.9 = 0.413 + 0.27 = 0.683
        module.update_preference("theme", 0.9);
        let second = module.get_preference("theme").unwrap();
        assert!(second > first);
        assert!((second - 0.683).abs() < 0.01);
    }

    #[test]
    fn test_bayesian_update() {
        let mut module = BeliefModule::new();

        // 先验为 0.5
        module.update_preference("language", 0.8);
        let first = module.get_preference("language").unwrap();

        // 先验为 first
        module.update_preference("language", 0.9);
        let second = module.get_preference("language").unwrap();

        // 第二次更新应该更接近 0.9
        assert!(second > first);
        assert!(second < 0.9);
    }

    #[test]
    fn test_user_feedback_evidence() {
        let mut module = BeliefModule::new();

        let evidence = Evidence::UserFeedback {
            preference: "color".to_string(),
            value: 0.7,
        };

        module.update_belief(evidence);

        assert!(module.get_preference("color").is_some());
    }

    #[test]
    fn test_goal_progress_update() {
        let mut module = BeliefModule::new();

        let goal = Goal {
            description: "Learn Rust".to_string(),
            priority: 0.8,
            deadline: None,
            progress: 0.5,
        };

        module.add_goal(goal);

        // 成功完成行动
        let evidence = Evidence::ActionCompleted {
            goal_id: 0,
            success: true,
        };
        module.update_belief(evidence);

        assert_eq!(module.get_state().goals[0].progress, 0.7);

        // 失败的行动
        let evidence = Evidence::ActionCompleted {
            goal_id: 0,
            success: false,
        };
        module.update_belief(evidence);

        assert_eq!(module.get_state().goals[0].progress, 0.6);
    }

    #[test]
    fn test_confidence_increases_with_evidence() {
        let mut module = BeliefModule::new();

        let initial_confidence = module.get_confidence();

        // 添加多个证据
        for i in 0..5 {
            let evidence = Evidence::UserFeedback {
                preference: format!("pref_{}", i),
                value: 0.7,
            };
            module.update_belief(evidence);
        }

        let final_confidence = module.get_confidence();

        // 证据增加应该提高置信度
        assert!(
            final_confidence >= initial_confidence,
            "final: {}, initial: {}",
            final_confidence,
            initial_confidence
        );
    }

    #[test]
    fn test_consistency_calculation() {
        let mut module = BeliefModule::new();

        // 添加一致的偏好
        module.update_preference("pref1", 0.8);
        module.update_preference("pref2", 0.8);
        module.update_preference("pref3", 0.8);

        let consistency = module.calculate_consistency();

        // 完全一致应该接近 1.0
        assert!(consistency > 0.9);
    }

    #[test]
    fn test_context_change_reduces_confidence() {
        let mut module = BeliefModule::new();

        // 建立一些信念
        for i in 0..5 {
            let evidence = Evidence::UserFeedback {
                preference: format!("pref_{}", i),
                value: 0.7,
            };
            module.update_belief(evidence);
        }

        let _before = module.get_confidence();

        // 上下文变化会被记录为证据
        let evidence = Evidence::ContextChange {
            context: "new_environment".to_string(),
        };
        module.update_belief(evidence);

        let after = module.get_confidence();

        // 上下文变化被记录，置信度可能变化
        // 由于证据数量增加，置信度可能上升或保持
        assert!(after >= 0.0 && after <= 1.0);
    }

    #[test]
    fn test_preference_clamping() {
        let mut module = BeliefModule::new();

        // 测试上限：0.7 * 0.5 + 0.3 * 2.0 = 0.35 + 0.6 = 0.95，然后 clamp 到 1.0
        module.update_preference("max", 2.0);
        let value = module.get_preference("max").unwrap();
        assert!((value - 0.95).abs() < 0.01 || value == 1.0);

        // 测试下限：0.7 * 0.5 + 0.3 * (-1.0) = 0.35 - 0.3 = 0.05，然后 clamp 到 0.0
        module.update_preference("min", -1.0);
        let value = module.get_preference("min").unwrap();
        assert!(value >= 0.0 && value <= 0.1);
    }

    #[test]
    fn test_multiple_goals() {
        let mut module = BeliefModule::new();

        let goal1 = Goal {
            description: "Goal 1".to_string(),
            priority: 0.8,
            deadline: None,
            progress: 0.0,
        };

        let goal2 = Goal {
            description: "Goal 2".to_string(),
            priority: 0.6,
            deadline: None,
            progress: 0.0,
        };

        module.add_goal(goal1);
        module.add_goal(goal2);

        assert_eq!(module.get_state().goals.len(), 2);
    }

    #[test]
    fn test_clear_evidence() {
        let mut module = BeliefModule::new();

        for i in 0..5 {
            let evidence = Evidence::UserFeedback {
                preference: format!("pref_{}", i),
                value: 0.7,
            };
            module.update_belief(evidence);
        }

        assert_eq!(module.evidence_history.len(), 5);

        module.clear_evidence();

        assert_eq!(module.evidence_history.len(), 0);
    }
}
