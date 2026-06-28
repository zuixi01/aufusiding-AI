//! 通知过滤规则
//!
//! 白名单/黑名单/时段/优先级过滤。

use serde::{Deserialize, Serialize};

use super::notification_hub::{Notification, NotificationPriority};

/// 过滤动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterAction {
    Allow,
    Block,
    Mute,
    Elevate,
}

/// 通知过滤规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRule {
    pub name: String,
    pub source_pattern: Option<String>,
    pub min_priority: Option<NotificationPriority>,
    pub action: FilterAction,
    pub enabled: bool,
}

/// 规则引擎
pub struct NotificationRuleEngine {
    rules: Vec<NotificationRule>,
}

impl NotificationRuleEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 添加规则
    pub fn add_rule(&mut self, rule: NotificationRule) {
        self.rules.push(rule);
    }

    /// 评估通知是否应该通过
    pub fn evaluate(&self, notification: &Notification) -> FilterAction {
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            let source_match = rule
                .source_pattern
                .as_ref()
                .map(|p| notification.source.contains(p))
                .unwrap_or(true);

            let priority_match = rule
                .min_priority
                .map(|min| notification.priority >= min)
                .unwrap_or(true);

            if source_match && priority_match {
                return rule.action;
            }
        }

        FilterAction::Allow
    }

    /// 规则数
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 移除规则
    pub fn remove_rule(&mut self, name: &str) {
        self.rules.retain(|r| r.name != name);
    }
}
