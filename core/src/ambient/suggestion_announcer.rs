//! 建议播报器
//!
//! 过滤、去重、格式化主动建议为 TTS 文本。

use chrono::{DateTime, Utc};

use crate::iot::semantic::proactive::ProactiveSuggestion;

/// 建议播报器
pub struct SuggestionAnnouncer {
    min_confidence: f64,
    cooldown_secs: u64,
    recent: Vec<(String, DateTime<Utc>)>,
}

impl SuggestionAnnouncer {
    pub fn new(min_confidence: f64, cooldown_secs: u64) -> Self {
        Self {
            min_confidence,
            cooldown_secs,
            recent: Vec::new(),
        }
    }

    /// 格式化建议为 TTS 文本
    pub fn format_for_tts(suggestion: &ProactiveSuggestion) -> String {
        format!("{}。原因：{}", suggestion.description, suggestion.reason)
    }

    /// 过滤低置信度建议
    pub fn filter(
        suggestions: Vec<ProactiveSuggestion>,
        min_confidence: f64,
    ) -> Vec<ProactiveSuggestion> {
        suggestions
            .into_iter()
            .filter(|s| s.confidence >= min_confidence)
            .collect()
    }

    /// 判断是否应该播报（冷却期去重）
    pub fn should_announce(&mut self, suggestion: &ProactiveSuggestion) -> bool {
        let now = Utc::now();
        // 清理过期记录
        self.recent
            .retain(|(_, ts)| (now - *ts).num_seconds() < self.cooldown_secs as i64);
        // 检查是否在冷却期内
        let key = &suggestion.description;
        if self.recent.iter().any(|(k, _)| k == key) {
            return false;
        }
        self.recent.push((key.clone(), now));
        true
    }

    /// 处理一批建议，返回应播报的 TTS 文本列表
    pub fn process(&mut self, suggestions: Vec<ProactiveSuggestion>) -> Vec<String> {
        let filtered = Self::filter(suggestions, self.min_confidence);
        filtered
            .into_iter()
            .filter(|s| self.should_announce(s))
            .map(|s| Self::format_for_tts(&s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::scenes::SceneAction;
    use crate::iot::semantic::proactive::{ProactiveSuggestion, SuggestionCategory};

    fn make_suggestion(desc: &str, confidence: f64) -> ProactiveSuggestion {
        ProactiveSuggestion {
            description: desc.to_string(),
            actions: vec![SceneAction {
                device_id: "light-001".into(),
                command: "turn_off".into(),
                params: serde_json::json!({}),
                delay_ms: 0,
            }],
            confidence,
            reason: "测试原因".into(),
            category: SuggestionCategory::Energy,
        }
    }

    #[test]
    fn format_for_tts_includes_description_and_reason() {
        let s = make_suggestion("客厅灯已开启但无人", 0.8);
        let text = SuggestionAnnouncer::format_for_tts(&s);
        assert!(text.contains("客厅灯已开启但无人"));
        assert!(text.contains("测试原因"));
    }

    #[test]
    fn filter_removes_low_confidence() {
        let suggestions = vec![
            make_suggestion("高置信度", 0.9),
            make_suggestion("低置信度", 0.3),
            make_suggestion("中置信度", 0.6),
        ];
        let filtered = SuggestionAnnouncer::filter(suggestions, 0.5);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|s| s.confidence >= 0.5));
    }

    #[test]
    fn should_announce_first_time_returns_true() {
        let mut announcer = SuggestionAnnouncer::new(0.5, 300);
        let s = make_suggestion("测试建议", 0.8);
        assert!(announcer.should_announce(&s));
    }

    #[test]
    fn should_announce_duplicate_in_cooldown_returns_false() {
        let mut announcer = SuggestionAnnouncer::new(0.5, 300);
        let s = make_suggestion("测试建议", 0.8);
        assert!(announcer.should_announce(&s));
        assert!(!announcer.should_announce(&s));
    }

    #[test]
    fn should_announce_different_suggestion_returns_true() {
        let mut announcer = SuggestionAnnouncer::new(0.5, 300);
        let s1 = make_suggestion("建议A", 0.8);
        let s2 = make_suggestion("建议B", 0.8);
        assert!(announcer.should_announce(&s1));
        assert!(announcer.should_announce(&s2));
    }

    #[test]
    fn should_announce_after_cooldown_returns_true() {
        let mut announcer = SuggestionAnnouncer::new(0.5, 0); // 0秒冷却
        let s = make_suggestion("测试建议", 0.8);
        assert!(announcer.should_announce(&s));
        // 冷却期为0，清理后应可再次播报
        assert!(announcer.should_announce(&s));
    }

    #[test]
    fn process_filters_and_deduplicates() {
        let mut announcer = SuggestionAnnouncer::new(0.5, 300);
        let suggestions = vec![
            make_suggestion("高置信度建议", 0.9),
            make_suggestion("低置信度建议", 0.3),
        ];
        let texts = announcer.process(suggestions);
        assert_eq!(texts.len(), 1);
        assert!(texts[0].contains("高置信度建议"));
    }

    #[test]
    fn process_deduplicates_across_calls() {
        let mut announcer = SuggestionAnnouncer::new(0.5, 300);
        let s1 = vec![make_suggestion("重复建议", 0.9)];
        let s2 = vec![make_suggestion("重复建议", 0.9)];
        let t1 = announcer.process(s1);
        let t2 = announcer.process(s2);
        assert_eq!(t1.len(), 1);
        assert_eq!(t2.len(), 0);
    }
}
