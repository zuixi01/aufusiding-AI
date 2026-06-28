//! 定时调度器
//!
//! 周期性检查 Schedule 触发器的 cron 表达式，匹配时触发规则。

use cron::Schedule;
use std::str::FromStr;
use std::sync::Arc;

use super::automation::AutomationEngine;
use super::device_state_cache::DeviceStateCache;

/// Cron 调度器 — 驱动 Schedule 类型触发器
pub struct CronScheduler {
    engine: Arc<AutomationEngine>,
    state_cache: Arc<DeviceStateCache>,
}

impl CronScheduler {
    pub fn new(engine: Arc<AutomationEngine>, state_cache: Arc<DeviceStateCache>) -> Self {
        Self { engine, state_cache }
    }

    /// 启动后台调度循环，每 interval_secs 秒检查一次
    pub async fn start(&self, interval_secs: u64) {
        let engine = Arc::clone(&self.engine);
        let state_cache = Arc::clone(&self.state_cache);
        let interval = tokio::time::Duration::from_secs(interval_secs);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                Self::check_schedules(&engine, &state_cache).await;
            }
        });
    }

    /// 单次检查所有 Schedule 规则（可测试）
    pub async fn check_schedules(engine: &AutomationEngine, state_cache: &DeviceStateCache) {
        let rules = engine.list_rules().await;
        let now = chrono::Utc::now();
        let device_states = state_cache.snapshot().await;

        for rule in &rules {
            if !rule.enabled {
                continue;
            }

            let has_schedule_trigger = rule.triggers.iter().any(|t| {
                if let super::automation::Trigger::Schedule { cron } = t {
                    if let Ok(schedule) = Schedule::from_str(cron) {
                        // Check if next occurrence is within 60 seconds (one check window)
                        if let Some(next) = schedule.upcoming(chrono::Utc).next() {
                            let diff = next.signed_duration_since(now).num_seconds();
                            return diff >= 0 && diff < 60;
                        }
                    }
                    false
                } else {
                    false
                }
            });

            if !has_schedule_trigger {
                continue;
            }

            // Evaluate conditions
            let conditions_met = rule.conditions.iter()
                .all(|c| AutomationEngine::evaluate_condition(c, &device_states));
            if !conditions_met {
                continue;
            }

            if let Err(e) = engine.trigger_rule(&rule.id).await {
                tracing::warn!("cron trigger_rule failed for {}: {}", rule.id, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::automation::{Action, AutomationRule, NotifyLevel, Trigger};
    use crate::iot::event_bus::EventBus;

    fn make_schedule_rule(id: &str, cron: &str) -> AutomationRule {
        AutomationRule {
            id: id.to_string(),
            name: format!("Schedule {}", id),
            enabled: true,
            triggers: vec![Trigger::Schedule { cron: cron.to_string() }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: format!("cron fired {}", id),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        }
    }

    #[tokio::test]
    async fn cron_matching_triggers_rule() {
        let bus = Arc::new(EventBus::new(64));
        let engine = Arc::new(AutomationEngine::new(bus));
        let cache = Arc::new(DeviceStateCache::new());

        // "every second" cron — next occurrence is always within 60s
        let rule = make_schedule_rule("r1", "* * * * * * *");
        engine.add_rule(rule).await.unwrap();

        CronScheduler::check_schedules(&engine, &cache).await;

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_some());
        let notifications = engine.notifications().await;
        assert_eq!(notifications.len(), 1);
    }

    #[tokio::test]
    async fn cron_not_matching_does_not_trigger() {
        let bus = Arc::new(EventBus::new(64));
        let engine = Arc::new(AutomationEngine::new(bus));
        let cache = Arc::new(DeviceStateCache::new());

        // Feb 30 never exists
        let rule = make_schedule_rule("r1", "0 0 0 30 2 * 2099");
        engine.add_rule(rule).await.unwrap();

        CronScheduler::check_schedules(&engine, &cache).await;

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_none());
    }

    #[tokio::test]
    async fn multiple_cron_rules_all_match() {
        let bus = Arc::new(EventBus::new(64));
        let engine = Arc::new(AutomationEngine::new(bus));
        let cache = Arc::new(DeviceStateCache::new());

        engine.add_rule(make_schedule_rule("r1", "* * * * * * *")).await.unwrap();
        engine.add_rule(make_schedule_rule("r2", "* * * * * * *")).await.unwrap();

        CronScheduler::check_schedules(&engine, &cache).await;

        let notifications = engine.notifications().await;
        assert_eq!(notifications.len(), 2);
    }

    #[tokio::test]
    async fn no_schedule_rules_no_error() {
        let bus = Arc::new(EventBus::new(64));
        let engine = Arc::new(AutomationEngine::new(bus));
        let cache = Arc::new(DeviceStateCache::new());

        // Add a non-schedule rule
        let rule = AutomationRule {
            id: "event-rule".into(),
            name: "Event rule".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "d1".into(),
                event_kind: crate::iot::event_bus::EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();

        CronScheduler::check_schedules(&engine, &cache).await;
        // No panic, no trigger
        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_none());
    }

    #[tokio::test]
    async fn disabled_schedule_rule_skipped() {
        let bus = Arc::new(EventBus::new(64));
        let engine = Arc::new(AutomationEngine::new(bus));
        let cache = Arc::new(DeviceStateCache::new());

        let mut rule = make_schedule_rule("r1", "* * * * * * *");
        rule.enabled = false;
        engine.add_rule(rule).await.unwrap();

        CronScheduler::check_schedules(&engine, &cache).await;

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_none());
    }
}
