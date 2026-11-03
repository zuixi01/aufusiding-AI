//! PBRIC 预测模块
//!
//! 基于历史行为的用户意图预测，使用 n-gram 马尔可夫链模型。

use chrono::Timelike;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// 用户行为类型
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum UserAction {
    /// 提问
    AskQuestion,
    /// 请求代码
    RequestCode,
    /// 搜索知识
    SearchKnowledge,
    /// 设置提醒
    SetReminder,
    /// 控制设备
    ControlDevice,
    /// 其他
    Other(String),
}

/// 预测模块
pub struct PredictModule {
    /// 行为历史（最近 N 个）
    action_history: VecDeque<UserAction>,
    /// n-gram 模型
    ngram_model: NgramModel,
    /// 最大历史长度
    max_history: usize,
}

/// N-gram 模型
struct NgramModel {
    /// n-gram 计数：(context, action) -> count
    counts: HashMap<Vec<UserAction>, HashMap<UserAction, usize>>,
    /// 平滑参数
    alpha: f64,
}

impl NgramModel {
    fn new(alpha: f64) -> Self {
        Self {
            counts: HashMap::new(),
            alpha,
        }
    }

    /// 记录行为序列
    fn record(&mut self, actions: &[UserAction]) {
        if actions.len() < 2 {
            return;
        }

        // 记录 2-gram 和 3-gram
        for window_size in 2..=3.min(actions.len()) {
            for i in 0..=actions.len() - window_size {
                let context = actions[i..i + window_size - 1].to_vec();
                let action = actions[i + window_size - 1].clone();

                self.counts
                    .entry(context)
                    .or_default()
                    .entry(action)
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
            }
        }
    }

    /// 预测下一个行为
    fn predict(&self, context: &[UserAction]) -> Vec<(UserAction, f64)> {
        // 尝试不同长度的上下文（从长到短）
        for n in (1..=context.len().min(2)).rev() {
            let ctx = context[context.len() - n..].to_vec();

            if let Some(action_counts) = self.counts.get(&ctx) {
                let total: usize = action_counts.values().sum();
                let vocab_size = self.get_vocab_size();

                let mut predictions: Vec<(UserAction, f64)> = action_counts
                    .iter()
                    .map(|(action, &count)| {
                        // Laplace 平滑
                        let prob = (count as f64 + self.alpha)
                            / (total as f64 + self.alpha * vocab_size as f64);
                        (action.clone(), prob)
                    })
                    .collect();

                predictions
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                return predictions;
            }
        }

        // 如果没有匹配的上下文，返回默认预测
        self.get_default_predictions()
    }

    fn get_vocab_size(&self) -> usize {
        let mut actions = std::collections::HashSet::new();
        for action_counts in self.counts.values() {
            for action in action_counts.keys() {
                actions.insert(action.clone());
            }
        }
        actions.len().max(5) // 至少假设有 5 种行为
    }

    fn get_default_predictions(&self) -> Vec<(UserAction, f64)> {
        vec![
            (UserAction::AskQuestion, 0.4),
            (UserAction::SearchKnowledge, 0.3),
            (UserAction::RequestCode, 0.2),
            (UserAction::Other("unknown".to_string()), 0.1),
        ]
    }
}

impl PredictModule {
    /// 创建预测模块
    pub fn new() -> Self {
        Self {
            action_history: VecDeque::new(),
            ngram_model: NgramModel::new(0.1),
            max_history: 100,
        }
    }

    /// 记录用户行为
    pub fn record_action(&mut self, action: UserAction) {
        self.action_history.push_back(action.clone());

        if self.action_history.len() > self.max_history {
            self.action_history.pop_front();
        }

        // 更新 n-gram 模型
        let recent: Vec<UserAction> = self.action_history.iter().cloned().collect();
        self.ngram_model.record(&recent);
    }

    /// 预测下一个行为
    pub fn predict_next_action(&self) -> Vec<(UserAction, f64)> {
        if self.action_history.len() < 2 {
            return self.ngram_model.get_default_predictions();
        }

        // 获取最近的行为作为上下文
        let recent: Vec<UserAction> = self
            .action_history
            .iter()
            .rev()
            .take(3)
            .rev()
            .cloned()
            .collect();

        let predictions = self.ngram_model.predict(&recent);

        // 根据上下文调整
        self.adjust_by_context(predictions)
    }

    /// 根据上下文调整预测
    fn adjust_by_context(&self, mut predictions: Vec<(UserAction, f64)>) -> Vec<(UserAction, f64)> {
        // 时间上下文调整
        let hour = chrono::Local::now().hour();

        for (action, prob) in &mut predictions {
            match action {
                UserAction::ControlDevice => {
                    // 夜间降低设备控制概率
                    if hour >= 22 || hour <= 6 {
                        *prob *= 0.5;
                    }
                }
                UserAction::RequestCode if (9..=18).contains(&hour) => {
                    // 工作时间提高代码请求概率
                    *prob *= 1.2;
                }
                _ => {}
            }
        }

        // 归一化
        let sum: f64 = predictions.iter().map(|(_, p)| p).sum();
        if sum > 0.0 {
            for (_, prob) in &mut predictions {
                *prob /= sum;
            }
        }

        predictions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        predictions
    }

    /// 获取行为历史
    pub fn get_history(&self) -> Vec<UserAction> {
        self.action_history.iter().cloned().collect()
    }

    /// 清空历史
    pub fn clear_history(&mut self) {
        self.action_history.clear();
    }
}

impl Default for PredictModule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_action() {
        let mut module = PredictModule::new();

        module.record_action(UserAction::AskQuestion);
        module.record_action(UserAction::SearchKnowledge);

        assert_eq!(module.get_history().len(), 2);
    }

    #[test]
    fn test_predict_with_pattern() {
        let mut module = PredictModule::new();

        // 建立模式：问问题 -> 搜索知识
        for _ in 0..5 {
            module.record_action(UserAction::AskQuestion);
            module.record_action(UserAction::SearchKnowledge);
        }

        // 现在问问题后，应该预测搜索知识
        module.record_action(UserAction::AskQuestion);
        let predictions = module.predict_next_action();

        assert!(!predictions.is_empty());
        // 第一个预测应该是 SearchKnowledge
        assert!(matches!(predictions[0].0, UserAction::SearchKnowledge));
    }

    #[test]
    fn test_ngram_model() {
        let mut model = NgramModel::new(0.1);

        let actions = vec![
            UserAction::AskQuestion,
            UserAction::SearchKnowledge,
            UserAction::AskQuestion,
            UserAction::SearchKnowledge,
        ];

        model.record(&actions);

        let predictions = model.predict(&[UserAction::AskQuestion]);
        assert!(!predictions.is_empty());
    }

    #[test]
    fn test_default_predictions() {
        let module = PredictModule::new();

        let predictions = module.predict_next_action();

        assert!(!predictions.is_empty());
        assert!(predictions[0].1 > 0.0);
    }

    #[test]
    fn test_context_adjustment() {
        let mut module = PredictModule::new();

        module.record_action(UserAction::ControlDevice);

        let predictions = vec![
            (UserAction::ControlDevice, 0.5),
            (UserAction::AskQuestion, 0.5),
        ];

        let adjusted = module.adjust_by_context(predictions);

        // 概率和应该为 1.0
        let sum: f64 = adjusted.iter().map(|(_, p)| p).sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_max_history_limit() {
        let mut module = PredictModule::new();
        module.max_history = 10;

        // 添加超过限制的行为
        for i in 0..20 {
            module.record_action(UserAction::Other(format!("action_{}", i)));
        }

        assert_eq!(module.get_history().len(), 10);
    }

    #[test]
    fn test_clear_history() {
        let mut module = PredictModule::new();

        module.record_action(UserAction::AskQuestion);
        module.record_action(UserAction::SearchKnowledge);

        assert_eq!(module.get_history().len(), 2);

        module.clear_history();

        assert_eq!(module.get_history().len(), 0);
    }

    #[test]
    fn test_multiple_patterns() {
        let mut module = PredictModule::new();

        // 模式 1: 问问题 -> 搜索
        for _ in 0..3 {
            module.record_action(UserAction::AskQuestion);
            module.record_action(UserAction::SearchKnowledge);
        }

        // 模式 2: 请求代码 -> 问问题
        for _ in 0..3 {
            module.record_action(UserAction::RequestCode);
            module.record_action(UserAction::AskQuestion);
        }

        // 测试模式 1
        module.record_action(UserAction::AskQuestion);
        let predictions = module.predict_next_action();
        assert!(matches!(predictions[0].0, UserAction::SearchKnowledge));

        // 测试模式 2
        module.record_action(UserAction::RequestCode);
        let predictions = module.predict_next_action();
        assert!(matches!(predictions[0].0, UserAction::AskQuestion));
    }

    #[test]
    fn test_laplace_smoothing() {
        let mut model = NgramModel::new(0.1);

        let actions = vec![UserAction::AskQuestion, UserAction::SearchKnowledge];

        model.record(&actions);

        // 即使没见过的上下文也应该有预测
        let predictions = model.predict(&[UserAction::ControlDevice]);
        assert!(!predictions.is_empty());
    }

    #[test]
    fn test_probability_sum() {
        let mut module = PredictModule::new();

        for _ in 0..5 {
            module.record_action(UserAction::AskQuestion);
            module.record_action(UserAction::SearchKnowledge);
        }

        let predictions = module.predict_next_action();

        let sum: f64 = predictions.iter().map(|(_, p)| p).sum();
        assert!((sum - 1.0).abs() < 0.01);
    }
}
