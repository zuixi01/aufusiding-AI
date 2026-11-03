//! 习惯学习引擎
//!
//! 分析用户行为模式，生成自动化建议。

use super::routine_store::{RoutineStore, UserEvent};

/// 习惯模式
#[derive(Debug, Clone)]
pub struct RoutinePattern {
    pub description: String,
    pub event_type: String,
    pub occurrences: usize,
    pub confidence: f64,
    /// 典型发生时间（小时:分钟）
    pub typical_time: Option<String>,
}

/// 自动化建议
#[derive(Debug, Clone)]
pub struct AutomationSuggestion {
    pub pattern: RoutinePattern,
    pub explanation: String,
}

/// 习惯学习引擎
pub struct RoutineLearner {
    store: RoutineStore,
    /// 至少出现 N 次才算习惯
    min_occurrences: usize,
    /// 置信度阈值
    confidence_threshold: f64,
}

impl RoutineLearner {
    pub fn new(min_occurrences: usize, confidence_threshold: f64) -> Self {
        Self {
            store: RoutineStore::new(10000),
            min_occurrences,
            confidence_threshold,
        }
    }

    /// 记录用户行为事件
    pub fn record_event(&mut self, event: UserEvent) {
        self.store.record(event);
    }

    /// 分析习惯模式
    pub fn analyze_patterns(&self) -> Vec<RoutinePattern> {
        let all_events = self.all_events();

        // 按事件类型分组
        let mut grouped: std::collections::HashMap<String, Vec<&UserEvent>> =
            std::collections::HashMap::new();
        for event in &all_events {
            grouped
                .entry(event.event_type.clone())
                .or_default()
                .push(event);
        }

        let mut patterns = Vec::new();
        for (event_type, events) in &grouped {
            let count = events.len();
            if count < self.min_occurrences {
                continue;
            }

            // 计算时间一致性作为置信度
            let confidence = self.time_consistency(events);
            if confidence < self.confidence_threshold {
                continue;
            }

            let typical_time = self.most_common_hour(events);

            patterns.push(RoutinePattern {
                description: format!("用户经常执行 '{}'", event_type),
                event_type: event_type.clone(),
                occurrences: count,
                confidence,
                typical_time,
            });
        }

        patterns.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        patterns
    }

    /// 生成自动化建议
    pub fn suggest_automations(&self) -> Vec<AutomationSuggestion> {
        self.analyze_patterns()
            .into_iter()
            .map(|pattern| {
                let time_str = pattern.typical_time.clone().unwrap_or("某个时间".into());
                let explanation = format!(
                    "我注意到您经常在{}执行'{}'（已重复 {} 次），要自动化这个操作吗？",
                    time_str, pattern.event_type, pattern.occurrences
                );
                AutomationSuggestion {
                    pattern,
                    explanation,
                }
            })
            .collect()
    }

    /// 已记录事件数
    pub fn event_count(&self) -> usize {
        self.store.count()
    }

    fn all_events(&self) -> Vec<UserEvent> {
        // 通过时间范围获取所有事件
        let far_past = chrono::DateTime::<chrono::Utc>::MIN_UTC;
        let far_future = chrono::DateTime::<chrono::Utc>::MAX_UTC;
        self.store
            .events_in_range(far_past, far_future)
            .into_iter()
            .cloned()
            .collect()
    }

    fn time_consistency(&self, events: &[&UserEvent]) -> f64 {
        if events.len() < 2 {
            return 0.0;
        }
        let hours: Vec<u32> = events
            .iter()
            .map(|e| e.timestamp.format("%H").to_string().parse().unwrap_or(0))
            .collect();
        let mean = hours.iter().sum::<u32>() as f64 / hours.len() as f64;
        let variance = hours
            .iter()
            .map(|h| (*h as f64 - mean).powi(2))
            .sum::<f64>()
            / hours.len() as f64;
        let std_dev = variance.sqrt();
        // 标准差越小，时间越一致，置信度越高
        (1.0 - std_dev / 12.0).max(0.0)
    }

    fn most_common_hour(&self, events: &[&UserEvent]) -> Option<String> {
        if events.is_empty() {
            return None;
        }
        let mut hour_counts: std::collections::HashMap<u32, usize> =
            std::collections::HashMap::new();
        for e in events {
            let hour: u32 = e.timestamp.format("%H").to_string().parse().unwrap_or(0);
            *hour_counts.entry(hour).or_default() += 1;
        }
        hour_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(hour, _)| format!("{:02}:00", hour))
    }
}
