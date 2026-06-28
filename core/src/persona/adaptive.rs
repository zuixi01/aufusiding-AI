//! 自适应学习引擎
//!
//! 基于用户反馈调整行为，使用 EMA 平滑反馈分数。

/// 风格调整建议
#[derive(Debug, Clone)]
pub struct StyleAdjustments {
    pub verbosity: f32,
    pub formality: f32,
    pub technical_level: f32,
    pub humor: f32,
}

impl Default for StyleAdjustments {
    fn default() -> Self {
        Self {
            verbosity: 0.5,
            formality: 0.5,
            technical_level: 0.5,
            humor: 0.3,
        }
    }
}

/// 反馈记录
#[derive(Debug, Clone)]
struct FeedbackEntry {
    _interaction_id: String,
    _score: f32,
}

/// 自适应学习引擎
pub struct AdaptiveLearner {
    feedbacks: Vec<FeedbackEntry>,
    ema_score: f32,
    ema_alpha: f32,
    style: StyleAdjustments,
}

impl AdaptiveLearner {
    pub fn new() -> Self {
        Self {
            feedbacks: Vec::new(),
            ema_score: 0.5,
            ema_alpha: 0.3,
            style: StyleAdjustments::default(),
        }
    }

    /// 记录用户反馈
    pub fn record_feedback(&mut self, interaction_id: &str, score: f32) {
        let clamped = score.clamp(0.0, 1.0);
        self.feedbacks.push(FeedbackEntry {
            _interaction_id: interaction_id.to_string(),
            _score: clamped,
        });
        // EMA 更新
        self.ema_score = self.ema_alpha * clamped + (1.0 - self.ema_alpha) * self.ema_score;
        self.update_style();
    }

    fn update_style(&mut self) {
        // 高分 → 保持当前风格；低分 → 调整
        if self.ema_score > 0.7 {
            // 用户满意，微调
            self.style.verbosity = (self.style.verbosity + 0.5) / 2.0;
        } else if self.ema_score < 0.3 {
            // 用户不满意，增加详细度和正式度
            self.style.verbosity = (self.style.verbosity + 0.7) / 2.0;
            self.style.formality = (self.style.formality + 0.7) / 2.0;
        }
    }

    /// 返回风格调整建议
    pub fn get_style_adjustments(&self) -> StyleAdjustments {
        self.style.clone()
    }

    /// 从历史对话学习偏好
    pub fn learn_from_history(&mut self, messages: &[String]) {
        for msg in messages {
            let lower = msg.to_lowercase();
            // 检测技术性
            let tech_words = ["code", "api", "function", "debug", "error", "代码", "函数"];
            let tech_count = tech_words.iter().filter(|w| lower.contains(*w)).count();
            if tech_count > 0 {
                self.style.technical_level = (self.style.technical_level + 0.8) / 2.0;
            }
            // 检测简洁偏好
            if msg.len() < 20 {
                self.style.verbosity = (self.style.verbosity + 0.3) / 2.0;
            }
        }
    }

    pub fn ema_score(&self) -> f32 {
        self.ema_score
    }

    pub fn feedback_count(&self) -> usize {
        self.feedbacks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_feedback_updates_ema() {
        let mut learner = AdaptiveLearner::new();
        learner.record_feedback("i1", 1.0);
        assert!(learner.ema_score() > 0.5);
        learner.record_feedback("i2", 0.0);
        let after_low = learner.ema_score();
        assert!(after_low < learner.ema_score() + 0.5); // decreased
    }

    #[test]
    fn style_adjustments_default() {
        let learner = AdaptiveLearner::new();
        let style = learner.get_style_adjustments();
        assert_eq!(style.verbosity, 0.5);
        assert_eq!(style.formality, 0.5);
    }

    #[test]
    fn learn_from_history_adjusts_technical() {
        let mut learner = AdaptiveLearner::new();
        learner.learn_from_history(&[
            "Please fix this code error".into(),
            "Debug the API function".into(),
        ]);
        let style = learner.get_style_adjustments();
        assert!(style.technical_level > 0.5);
    }

    #[test]
    fn ema_smoothing() {
        let mut learner = AdaptiveLearner::new();
        // All high scores
        for i in 0..10 {
            learner.record_feedback(&format!("i{}", i), 1.0);
        }
        assert!(learner.ema_score() > 0.9);
        // One low score shouldn't crash it
        learner.record_feedback("low", 0.0);
        assert!(learner.ema_score() > 0.5);
    }

    #[test]
    fn default_values() {
        let learner = AdaptiveLearner::new();
        assert_eq!(learner.feedback_count(), 0);
        assert!((learner.ema_score() - 0.5).abs() < f32::EPSILON);
    }
}
