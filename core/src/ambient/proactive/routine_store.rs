//! 习惯数据持久化
//!
//! 存储用户行为事件用于习惯分析。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 用户行为事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEvent {
    pub id: Uuid,
    pub event_type: String,
    pub description: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl UserEvent {
    pub fn new(event_type: &str, description: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type: event_type.into(),
            description: description.into(),
            timestamp: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// 习惯数据存储
pub struct RoutineStore {
    events: Vec<UserEvent>,
    max_events: usize,
}

impl RoutineStore {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Vec::new(),
            max_events,
        }
    }

    /// 记录事件
    pub fn record(&mut self, event: UserEvent) {
        self.events.push(event);
        // 超过上限时移除最旧的
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }
    }

    /// 获取指定类型的事件
    pub fn events_by_type(&self, event_type: &str) -> Vec<&UserEvent> {
        self.events
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    /// 获取时间范围内的事件
    pub fn events_in_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<&UserEvent> {
        self.events
            .iter()
            .filter(|e| e.timestamp >= from && e.timestamp <= to)
            .collect()
    }

    /// 事件总数
    pub fn count(&self) -> usize {
        self.events.len()
    }

    /// 清空
    pub fn clear(&mut self) {
        self.events.clear();
    }
}
