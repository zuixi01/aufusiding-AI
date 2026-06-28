//! 使用统计
//!
//! 按用户/按功能/按时段统计使用情况。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 使用记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub speaker_id: Option<Uuid>,
    pub feature: String,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: u64,
}

/// 使用统计器
pub struct UsageTracker {
    records: Vec<UsageRecord>,
    max_records: usize,
}

impl UsageTracker {
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Vec::new(),
            max_records,
        }
    }

    /// 记录使用
    pub fn record(&mut self, record: UsageRecord) {
        self.records.push(record);
        if self.records.len() > self.max_records {
            self.records.remove(0);
        }
    }

    /// 按功能统计使用次数
    pub fn count_by_feature(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for r in &self.records {
            *counts.entry(r.feature.clone()).or_default() += 1;
        }
        counts
    }

    /// 按用户统计使用次数
    pub fn count_by_speaker(&self) -> HashMap<Option<Uuid>, usize> {
        let mut counts = HashMap::new();
        for r in &self.records {
            *counts.entry(r.speaker_id).or_default() += 1;
        }
        counts
    }

    /// 总使用时长（毫秒）
    pub fn total_duration_ms(&self) -> u64 {
        self.records.iter().map(|r| r.duration_ms).sum()
    }

    /// 记录总数
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// 最常用功能
    pub fn most_used_feature(&self) -> Option<String> {
        self.count_by_feature()
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(feature, _)| feature)
    }

    /// 清空
    pub fn clear(&mut self) {
        self.records.clear();
    }
}
