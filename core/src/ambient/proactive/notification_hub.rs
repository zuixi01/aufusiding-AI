//! 通知聚合中心
//!
//! 通知聚合、过滤、优先级排序、语音摘要。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 通知优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// 通知
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub source: String,
    pub title: String,
    pub content: String,
    pub priority: NotificationPriority,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
}

impl Notification {
    pub fn new(source: &str, title: &str, content: &str, priority: NotificationPriority) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: source.into(),
            title: title.into(),
            content: content.into(),
            priority,
            timestamp: Utc::now(),
            read: false,
        }
    }
}

/// 通知中心
pub struct NotificationHub {
    queue: std::sync::Mutex<Vec<Notification>>,
}

impl NotificationHub {
    pub fn new() -> Self {
        Self {
            queue: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 推送通知
    pub fn push(&self, notification: Notification) {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).push(notification);
    }

    /// 待处理通知数
    pub fn pending_count(&self) -> usize {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|n| !n.read).count()
    }

    /// 总通知数
    pub fn total_count(&self) -> usize {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// 按优先级排序获取未读通知
    pub fn pending_sorted(&self) -> Vec<Notification> {
        let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let mut pending: Vec<Notification> = queue.iter().filter(|n| !n.read).cloned().collect();
        pending.sort_by(|a, b| b.priority.cmp(&a.priority));
        pending
    }

    /// 生成语音摘要（按优先级排序，合并同源）
    pub fn voice_summary(&self) -> String {
        let pending = self.pending_sorted();
        if pending.is_empty() {
            return "没有新通知。".into();
        }

        let count = pending.len();
        let critical = pending.iter().filter(|n| n.priority == NotificationPriority::Critical).count();

        let mut summary = format!("你有 {} 条新通知", count);
        if critical > 0 {
            summary.push_str(&format!("，其中 {} 条紧急", critical));
        }
        summary.push('。');

        // 播报前 3 条
        for (i, n) in pending.iter().take(3).enumerate() {
            summary.push_str(&format!("第{}条，来自{}：{}。", i + 1, n.source, n.title));
        }

        if count > 3 {
            summary.push_str(&format!("还有 {} 条未读。", count - 3));
        }

        summary
    }

    /// 标记已读
    pub fn dismiss(&self, ids: &[Uuid]) {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        for n in queue.iter_mut() {
            if ids.contains(&n.id) {
                n.read = true;
            }
        }
    }

    /// 全部标记已读
    pub fn dismiss_all(&self) {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        for n in queue.iter_mut() {
            n.read = true;
        }
    }

    /// 清除已读通知
    pub fn clear_read(&self) {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.retain(|n| !n.read);
    }
}
