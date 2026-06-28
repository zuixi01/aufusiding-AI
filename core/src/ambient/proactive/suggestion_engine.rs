//! 建议引擎
//!
//! 基于习惯和上下文生成主动建议，包括设备控制建议。

use super::routine_learner::{AutomationSuggestion, RoutineLearner};
use crate::iot::semantic::proactive::ProactiveSuggestion;

/// 建议类型
#[derive(Debug, Clone)]
pub enum SuggestionType {
    /// 自动化建议（基于习惯）
    Automation(AutomationSuggestion),
    /// 提醒建议
    Reminder(String),
    /// 节能建议
    EnergySaving(String),
    /// 健康建议
    Health(String),
    /// 设备动作建议（基于语义推理）
    DeviceAction(ProactiveSuggestion),
}

/// 建议引擎
pub struct SuggestionEngine {
    /// 最大建议数
    max_suggestions: usize,
}

impl SuggestionEngine {
    pub fn new(max_suggestions: usize) -> Self {
        Self { max_suggestions }
    }

    /// 从习惯学习器生成建议
    pub fn from_routines(&self, learner: &RoutineLearner) -> Vec<SuggestionType> {
        learner
            .suggest_automations()
            .into_iter()
            .take(self.max_suggestions)
            .map(SuggestionType::Automation)
            .collect()
    }

    /// 生成语音播报文本
    pub fn voice_text(&self, suggestions: &[SuggestionType]) -> String {
        if suggestions.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        for (i, s) in suggestions.iter().take(3).enumerate() {
            let text = match s {
                SuggestionType::Automation(a) => a.explanation.clone(),
                SuggestionType::Reminder(msg) => msg.clone(),
                SuggestionType::EnergySaving(msg) => msg.clone(),
                SuggestionType::Health(msg) => msg.clone(),
                SuggestionType::DeviceAction(da) => da.description.clone(),
            };
            parts.push(format!("建议{}：{}", i + 1, text));
        }

        parts.join(" ")
    }
}
