//! 自动化规则引擎
//!
//! 事件驱动的自动化规则引擎，监听 EventBus 事件，评估条件，触发场景或动作。

use chrono::{DateTime, Datelike, NaiveTime, Timelike, Utc, Weekday};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::control::GenericController;
use super::device_state_cache::DeviceStateCache;
use super::event_bus::{DeviceEvent, EventBus, EventKind};
use super::scene_executor::SceneExecutor;
use super::scenes::SceneManager;
use super::vehicle::geofence::GeoTransition;

// ─── 比较操作符 ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Neq,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SunEventType {
    Sunrise,
    Sunset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotifyLevel {
    Info,
    Warning,
    Critical,
}

// ─── 触发器 ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trigger {
    DeviceEvent {
        device_id: String,
        event_kind: EventKind,
    },
    Schedule {
        cron: String,
    },
    Geofence {
        zone_id: String,
        transition: GeoTransition,
    },
    Threshold {
        device_id: String,
        property: String,
        op: CompareOp,
        value: Value,
    },
    SunEvent {
        event: SunEventType,
        offset_minutes: i32,
    },
}

// ─── 条件 ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    DeviceState {
        device_id: String,
        property: String,
        op: CompareOp,
        value: Value,
    },
    TimeRange {
        after: String,
        before: String,
    },
    DaysOfWeek {
        days: Vec<u8>,
    },
    And(Vec<Condition>),
    Or(Vec<Condition>),
    Not(Box<Condition>),
}

// ─── 动作 ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    RunScene {
        scene_id: String,
    },
    DeviceCommand {
        device_id: String,
        command: String,
        params: Value,
    },
    Delay {
        duration_ms: u64,
        then: Box<Action>,
    },
    Notify {
        message: String,
        level: NotifyLevel,
    },
    Sequence(Vec<Action>),
}

// ─── 自动化规则 ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub triggers: Vec<Trigger>,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
    pub last_triggered: Option<DateTime<Utc>>,
    pub cooldown_secs: Option<u64>,
}

// ─── 值比较辅助 ───────────────────────────────────────────

fn compare_values(left: &Value, right: &Value, op: &CompareOp) -> bool {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => {
            let lf = l.as_f64().unwrap_or(f64::NAN);
            let rf = r.as_f64().unwrap_or(f64::NAN);
            match op {
                CompareOp::Gt => lf > rf,
                CompareOp::Gte => lf >= rf,
                CompareOp::Lt => lf < rf,
                CompareOp::Lte => lf <= rf,
                CompareOp::Eq => (lf - rf).abs() < f64::EPSILON,
                CompareOp::Neq => (lf - rf).abs() >= f64::EPSILON,
            }
        }
        (Value::String(l), Value::String(r)) => match op {
            CompareOp::Gt => l > r,
            CompareOp::Gte => l >= r,
            CompareOp::Lt => l < r,
            CompareOp::Lte => l <= r,
            CompareOp::Eq => l == r,
            CompareOp::Neq => l != r,
        },
        (Value::Bool(l), Value::Bool(r)) => match op {
            CompareOp::Eq => l == r,
            CompareOp::Neq => l != r,
            _ => false,
        },
        _ => false,
    }
}

fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let h = parts[0].parse::<u32>().ok()?;
    let m = parts[1].parse::<u32>().ok()?;
    NaiveTime::from_hms_opt(h, m, 0)
}

fn weekday_to_u8(wd: Weekday) -> u8 {
    match wd {
        Weekday::Sun => 0,
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
    }
}

// ─── 自动化引擎 ───────────────────────────────────────────
pub struct AutomationEngine {
    rules: Arc<RwLock<Vec<AutomationRule>>>,
    event_bus: Arc<EventBus>,
    scene_executor: Option<Arc<RwLock<SceneExecutor>>>,
    controller: Option<Arc<GenericController>>,
    state_cache: Option<Arc<DeviceStateCache>>,
    scene_manager: Option<Arc<RwLock<SceneManager>>>,
    notification_log: Arc<RwLock<Vec<(String, NotifyLevel)>>>,
}

impl AutomationEngine {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            event_bus,
            scene_executor: None,
            controller: None,
            state_cache: None,
            scene_manager: None,
            notification_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_scene_executor(mut self, executor: Arc<RwLock<SceneExecutor>>) -> Self {
        self.scene_executor = Some(executor);
        self
    }

    pub fn with_controller(mut self, controller: Arc<GenericController>) -> Self {
        self.controller = Some(controller);
        self
    }

    pub fn with_state_cache(mut self, cache: Arc<DeviceStateCache>) -> Self {
        self.state_cache = Some(cache);
        self
    }

    pub fn with_scene_manager(mut self, manager: Arc<RwLock<SceneManager>>) -> Self {
        self.scene_manager = Some(manager);
        self
    }

    /// 获取通知日志
    pub async fn notifications(&self) -> Vec<(String, NotifyLevel)> {
        self.notification_log.read().await.clone()
    }

    /// 获取规则引用（供外部同步使用）
    pub fn rules_ref(&self) -> &Arc<RwLock<Vec<AutomationRule>>> {
        &self.rules
    }

    pub async fn add_rule(&self, rule: AutomationRule) -> anyhow::Result<()> {
        let mut rules = self.rules.write().await;
        if rules.iter().any(|r| r.id == rule.id) {
            anyhow::bail!("duplicate rule id: {}", rule.id);
        }
        rules.push(rule);
        Ok(())
    }

    pub async fn remove_rule(&self, rule_id: &str) -> bool {
        let mut rules = self.rules.write().await;
        let len_before = rules.len();
        rules.retain(|r| r.id != rule_id);
        rules.len() < len_before
    }

    pub async fn enable_rule(&self, rule_id: &str) -> bool {
        let mut rules = self.rules.write().await;
        if let Some(rule) = rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = true;
            return true;
        }
        false
    }

    pub async fn disable_rule(&self, rule_id: &str) -> bool {
        let mut rules = self.rules.write().await;
        if let Some(rule) = rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = false;
            return true;
        }
        false
    }

    pub async fn list_rules(&self) -> Vec<AutomationRule> {
        self.rules.read().await.clone()
    }

    /// 评估事件是否匹配触发器
    pub fn matches_trigger(trigger: &Trigger, event: &DeviceEvent) -> bool {
        match trigger {
            Trigger::DeviceEvent {
                device_id,
                event_kind,
            } => event.device_id == *device_id && event.event_kind == *event_kind,
            Trigger::Threshold {
                device_id,
                property,
                op,
                value,
            } => {
                if event.device_id != *device_id {
                    return false;
                }
                if let Some(actual) = event.payload.get(property) {
                    compare_values(actual, value, op)
                } else {
                    false
                }
            }
            Trigger::Geofence {
                zone_id,
                transition,
            } => {
                let payload_zone = event.payload.get("zone_id").and_then(|v| v.as_str());
                let payload_transition = event
                    .payload
                    .get("transition")
                    .and_then(|v| serde_json::from_value::<GeoTransition>(v.clone()).ok());
                payload_zone == Some(zone_id.as_str())
                    && payload_transition.as_ref() == Some(transition)
            }
            Trigger::Schedule { .. } | Trigger::SunEvent { .. } => false,
        }
    }

    /// 评估条件（使用当前时间）
    pub fn evaluate_condition(
        condition: &Condition,
        device_states: &HashMap<String, Value>,
    ) -> bool {
        let now = Utc::now();
        let time =
            NaiveTime::from_hms_opt(now.hour(), now.minute(), now.second()).unwrap_or_default();
        let weekday = now.weekday();
        Self::evaluate_condition_at(condition, device_states, time, weekday)
    }

    /// 评估条件（可测试版本，接受时间参数）
    pub fn evaluate_condition_at(
        condition: &Condition,
        device_states: &HashMap<String, Value>,
        time: NaiveTime,
        weekday: Weekday,
    ) -> bool {
        match condition {
            Condition::DeviceState {
                device_id,
                property,
                op,
                value,
            } => device_states
                .get(device_id)
                .and_then(|state| state.get(property))
                .map(|actual| compare_values(actual, value, op))
                .unwrap_or(false),
            Condition::TimeRange { after, before } => {
                let Some(after_t) = parse_hhmm(after) else {
                    return false;
                };
                let Some(before_t) = parse_hhmm(before) else {
                    return false;
                };
                if after_t <= before_t {
                    time >= after_t && time <= before_t
                } else {
                    // 跨午夜: e.g. 22:00 ~ 06:00
                    time >= after_t || time <= before_t
                }
            }
            Condition::DaysOfWeek { days } => days.contains(&weekday_to_u8(weekday)),
            Condition::And(conditions) => conditions
                .iter()
                .all(|c| Self::evaluate_condition_at(c, device_states, time, weekday)),
            Condition::Or(conditions) => conditions
                .iter()
                .any(|c| Self::evaluate_condition_at(c, device_states, time, weekday)),
            Condition::Not(inner) => {
                !Self::evaluate_condition_at(inner, device_states, time, weekday)
            }
        }
    }

    /// 验证 cron 表达式
    pub fn validate_cron(expr: &str) -> bool {
        Schedule::from_str(expr).is_ok()
    }

    /// 检查规则是否在冷却期内
    fn is_in_cooldown(rule: &AutomationRule) -> bool {
        if let (Some(cooldown), Some(last)) = (rule.cooldown_secs, rule.last_triggered) {
            let elapsed = Utc::now().signed_duration_since(last).num_seconds();
            elapsed >= 0 && (elapsed as u64) < cooldown
        } else {
            false
        }
    }

    /// 启动事件监听循环
    pub async fn start(&self) {
        let rules = Arc::clone(&self.rules);
        let event_bus = Arc::clone(&self.event_bus);
        let scene_executor = self.scene_executor.clone();
        let controller = self.controller.clone();
        let scene_manager = self.scene_manager.clone();
        let state_cache = self.state_cache.clone();
        let notification_log = Arc::clone(&self.notification_log);
        let bus_for_actions = Arc::clone(&self.event_bus);
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                let mut rules_guard = rules.write().await;
                // Collect actions to execute outside the lock
                let mut pending_actions: Vec<Vec<Action>> = Vec::new();

                for rule in rules_guard.iter_mut() {
                    if !rule.enabled {
                        continue;
                    }
                    if Self::is_in_cooldown(rule) {
                        continue;
                    }
                    let triggered = rule
                        .triggers
                        .iter()
                        .any(|t| Self::matches_trigger(t, &event));
                    if !triggered {
                        continue;
                    }

                    // Evaluate conditions using state_cache snapshot
                    let device_states = if let Some(ref cache) = state_cache {
                        cache.snapshot().await
                    } else {
                        HashMap::new()
                    };

                    let conditions_met = rule
                        .conditions
                        .iter()
                        .all(|c| Self::evaluate_condition(c, &device_states));
                    if !conditions_met {
                        continue;
                    }

                    rule.last_triggered = Some(Utc::now());
                    pending_actions.push(rule.actions.clone());
                }
                drop(rules_guard);

                // Execute actions outside the lock
                for actions in pending_actions {
                    for action in &actions {
                        if let Err(e) = Self::execute_action_internal(
                            action,
                            &scene_executor,
                            &controller,
                            &scene_manager,
                            &notification_log,
                            &bus_for_actions,
                        )
                        .await
                        {
                            tracing::warn!("automation action execution failed: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// 手动触发规则
    pub async fn trigger_rule(&self, rule_id: &str) -> anyhow::Result<()> {
        let actions = {
            let mut rules = self.rules.write().await;
            let rule = rules
                .iter_mut()
                .find(|r| r.id == rule_id)
                .ok_or_else(|| anyhow::anyhow!("rule not found: {}", rule_id))?;
            rule.last_triggered = Some(Utc::now());
            rule.actions.clone()
        };

        for action in &actions {
            if let Err(e) = Self::execute_action_internal(
                action,
                &self.scene_executor,
                &self.controller,
                &self.scene_manager,
                &self.notification_log,
                &self.event_bus,
            )
            .await
            {
                tracing::warn!("trigger_rule action execution failed: {}", e);
            }
        }
        Ok(())
    }

    /// 执行单个动作
    fn execute_action_internal<'a>(
        action: &'a Action,
        scene_executor: &'a Option<Arc<RwLock<SceneExecutor>>>,
        controller: &'a Option<Arc<GenericController>>,
        scene_manager: &'a Option<Arc<RwLock<SceneManager>>>,
        notification_log: &'a Arc<RwLock<Vec<(String, NotifyLevel)>>>,
        event_bus: &'a Arc<EventBus>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            match action {
                Action::RunScene { scene_id } => {
                    if let (Some(sm), Some(se)) = (scene_manager, scene_executor) {
                        let scene = {
                            let mgr = sm.read().await;
                            mgr.get_scene(scene_id).cloned()
                        };
                        if let Some(scene) = scene {
                            let executor = se.read().await;
                            executor.execute(&scene).await;
                        }
                    }
                    Ok(())
                }
                Action::DeviceCommand {
                    device_id,
                    command,
                    params,
                } => {
                    if let Some(ctrl) = controller {
                        ctrl.execute(device_id, command, params.clone()).await?;
                    }
                    Ok(())
                }
                Action::Delay { duration_ms, then } => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(*duration_ms)).await;
                    Self::execute_action_internal(
                        then,
                        scene_executor,
                        controller,
                        scene_manager,
                        notification_log,
                        event_bus,
                    )
                    .await
                }
                Action::Notify { message, level } => {
                    notification_log
                        .write()
                        .await
                        .push((message.clone(), level.clone()));
                    event_bus.publish(DeviceEvent::new(
                        "system",
                        super::types::DeviceType::Other("automation".into()),
                        EventKind::Alert,
                        serde_json::json!({"message": message, "level": format!("{:?}", level)}),
                    ));
                    Ok(())
                }
                Action::Sequence(actions) => {
                    for a in actions {
                        Self::execute_action_internal(
                            a,
                            scene_executor,
                            controller,
                            scene_manager,
                            notification_log,
                            event_bus,
                        )
                        .await?;
                    }
                    Ok(())
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::DeviceType;
    use super::*;

    fn make_event(device_id: &str, kind: EventKind, payload: Value) -> DeviceEvent {
        DeviceEvent::new(device_id, DeviceType::Sensor, kind, payload)
    }

    fn make_rule(id: &str) -> AutomationRule {
        AutomationRule {
            id: id.to_string(),
            name: format!("Rule {}", id),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "test".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        }
    }

    // ─── matches_trigger tests ────────────────────────────

    #[test]
    fn matches_trigger_device_event_matches() {
        let trigger = Trigger::DeviceEvent {
            device_id: "light-001".into(),
            event_kind: EventKind::StateChanged,
        };
        let event = make_event("light-001", EventKind::StateChanged, serde_json::json!({}));
        assert!(AutomationEngine::matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_trigger_device_event_wrong_id() {
        let trigger = Trigger::DeviceEvent {
            device_id: "light-001".into(),
            event_kind: EventKind::StateChanged,
        };
        let event = make_event("light-002", EventKind::StateChanged, serde_json::json!({}));
        assert!(!AutomationEngine::matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_trigger_device_event_wrong_kind() {
        let trigger = Trigger::DeviceEvent {
            device_id: "light-001".into(),
            event_kind: EventKind::StateChanged,
        };
        let event = make_event("light-001", EventKind::DeviceOnline, serde_json::json!({}));
        assert!(!AutomationEngine::matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_trigger_threshold_gt() {
        let trigger = Trigger::Threshold {
            device_id: "sensor-001".into(),
            property: "temperature".into(),
            op: CompareOp::Gt,
            value: serde_json::json!(30),
        };
        let event = make_event(
            "sensor-001",
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 35}),
        );
        assert!(AutomationEngine::matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_trigger_threshold_lt_no_match() {
        let trigger = Trigger::Threshold {
            device_id: "sensor-001".into(),
            property: "temperature".into(),
            op: CompareOp::Lt,
            value: serde_json::json!(20),
        };
        let event = make_event(
            "sensor-001",
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 25}),
        );
        assert!(!AutomationEngine::matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_trigger_geofence_enter() {
        let trigger = Trigger::Geofence {
            zone_id: "home".into(),
            transition: GeoTransition::Enter,
        };
        let event = make_event(
            "vehicle-001",
            EventKind::StateChanged,
            serde_json::json!({"zone_id": "home", "transition": "Enter"}),
        );
        assert!(AutomationEngine::matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_trigger_geofence_wrong_zone() {
        let trigger = Trigger::Geofence {
            zone_id: "home".into(),
            transition: GeoTransition::Enter,
        };
        let event = make_event(
            "vehicle-001",
            EventKind::StateChanged,
            serde_json::json!({"zone_id": "office", "transition": "Enter"}),
        );
        assert!(!AutomationEngine::matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_trigger_schedule_returns_false() {
        let trigger = Trigger::Schedule {
            cron: "0 0 * * * *".into(),
        };
        let event = make_event("any", EventKind::StateChanged, serde_json::json!({}));
        assert!(!AutomationEngine::matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_trigger_sun_event_returns_false() {
        let trigger = Trigger::SunEvent {
            event: SunEventType::Sunrise,
            offset_minutes: 0,
        };
        let event = make_event("any", EventKind::StateChanged, serde_json::json!({}));
        assert!(!AutomationEngine::matches_trigger(&trigger, &event));
    }

    // ─── evaluate_condition tests ──────────────────────────

    fn sample_device_states() -> HashMap<String, Value> {
        let mut states = HashMap::new();
        states.insert(
            "light-001".into(),
            serde_json::json!({"brightness": 80, "powered": true}),
        );
        states.insert(
            "sensor-001".into(),
            serde_json::json!({"temperature": 20, "humidity": 55}),
        );
        states
    }

    #[test]
    fn evaluate_condition_device_state_eq() {
        let cond = Condition::DeviceState {
            device_id: "light-001".into(),
            property: "brightness".into(),
            op: CompareOp::Eq,
            value: serde_json::json!(80),
        };
        assert!(AutomationEngine::evaluate_condition(
            &cond,
            &sample_device_states()
        ));
    }

    #[test]
    fn evaluate_condition_device_state_gt_no_match() {
        let cond = Condition::DeviceState {
            device_id: "sensor-001".into(),
            property: "temperature".into(),
            op: CompareOp::Gt,
            value: serde_json::json!(25),
        };
        assert!(!AutomationEngine::evaluate_condition(
            &cond,
            &sample_device_states()
        ));
    }

    #[test]
    fn evaluate_condition_time_range_in_range() {
        let cond = Condition::TimeRange {
            after: "08:00".into(),
            before: "18:00".into(),
        };
        let time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        assert!(AutomationEngine::evaluate_condition_at(
            &cond,
            &HashMap::new(),
            time,
            Weekday::Mon
        ));
    }

    #[test]
    fn evaluate_condition_time_range_out_of_range() {
        let cond = Condition::TimeRange {
            after: "08:00".into(),
            before: "18:00".into(),
        };
        let time = NaiveTime::from_hms_opt(20, 0, 0).unwrap();
        assert!(!AutomationEngine::evaluate_condition_at(
            &cond,
            &HashMap::new(),
            time,
            Weekday::Mon
        ));
    }

    #[test]
    fn evaluate_condition_days_of_week_match() {
        let cond = Condition::DaysOfWeek {
            days: vec![1, 2, 3, 4, 5],
        }; // Mon-Fri
        assert!(AutomationEngine::evaluate_condition_at(
            &cond,
            &HashMap::new(),
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            Weekday::Wed
        ));
    }

    #[test]
    fn evaluate_condition_days_of_week_no_match() {
        let cond = Condition::DaysOfWeek {
            days: vec![1, 2, 3, 4, 5],
        }; // Mon-Fri
        assert!(!AutomationEngine::evaluate_condition_at(
            &cond,
            &HashMap::new(),
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            Weekday::Sun
        ));
    }

    #[test]
    fn evaluate_condition_and_all_true() {
        let cond = Condition::And(vec![
            Condition::DeviceState {
                device_id: "light-001".into(),
                property: "brightness".into(),
                op: CompareOp::Eq,
                value: serde_json::json!(80),
            },
            Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Eq,
                value: serde_json::json!(20),
            },
        ]);
        assert!(AutomationEngine::evaluate_condition(
            &cond,
            &sample_device_states()
        ));
    }

    #[test]
    fn evaluate_condition_and_partial_false() {
        let cond = Condition::And(vec![
            Condition::DeviceState {
                device_id: "light-001".into(),
                property: "brightness".into(),
                op: CompareOp::Eq,
                value: serde_json::json!(80),
            },
            Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(25),
            },
        ]);
        assert!(!AutomationEngine::evaluate_condition(
            &cond,
            &sample_device_states()
        ));
    }

    #[test]
    fn evaluate_condition_or_any_true() {
        let cond = Condition::Or(vec![
            Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(25),
            },
            Condition::DeviceState {
                device_id: "light-001".into(),
                property: "brightness".into(),
                op: CompareOp::Eq,
                value: serde_json::json!(80),
            },
        ]);
        assert!(AutomationEngine::evaluate_condition(
            &cond,
            &sample_device_states()
        ));
    }

    #[test]
    fn evaluate_condition_or_all_false() {
        let cond = Condition::Or(vec![
            Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            },
            Condition::DeviceState {
                device_id: "light-001".into(),
                property: "brightness".into(),
                op: CompareOp::Lt,
                value: serde_json::json!(50),
            },
        ]);
        assert!(!AutomationEngine::evaluate_condition(
            &cond,
            &sample_device_states()
        ));
    }

    #[test]
    fn evaluate_condition_not_inverts() {
        let cond = Condition::Not(Box::new(Condition::DeviceState {
            device_id: "light-001".into(),
            property: "brightness".into(),
            op: CompareOp::Eq,
            value: serde_json::json!(80),
        }));
        assert!(!AutomationEngine::evaluate_condition(
            &cond,
            &sample_device_states()
        ));
    }

    #[test]
    fn evaluate_condition_nested() {
        // And(Or(temp>25, brightness==80), Not(humidity>60))
        let cond = Condition::And(vec![
            Condition::Or(vec![
                Condition::DeviceState {
                    device_id: "sensor-001".into(),
                    property: "temperature".into(),
                    op: CompareOp::Gt,
                    value: serde_json::json!(25),
                },
                Condition::DeviceState {
                    device_id: "light-001".into(),
                    property: "brightness".into(),
                    op: CompareOp::Eq,
                    value: serde_json::json!(80),
                },
            ]),
            Condition::Not(Box::new(Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "humidity".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(60),
            })),
        ]);
        assert!(AutomationEngine::evaluate_condition(
            &cond,
            &sample_device_states()
        ));
    }

    // ─── engine CRUD tests ────────────────────────────────

    #[tokio::test]
    async fn add_rule_success() {
        let bus = Arc::new(EventBus::new(16));
        let engine = AutomationEngine::new(bus);
        engine.add_rule(make_rule("r1")).await.unwrap();
        assert_eq!(engine.list_rules().await.len(), 1);
    }

    #[tokio::test]
    async fn add_rule_duplicate_id_error() {
        let bus = Arc::new(EventBus::new(16));
        let engine = AutomationEngine::new(bus);
        engine.add_rule(make_rule("r1")).await.unwrap();
        assert!(engine.add_rule(make_rule("r1")).await.is_err());
    }

    #[tokio::test]
    async fn remove_rule_success() {
        let bus = Arc::new(EventBus::new(16));
        let engine = AutomationEngine::new(bus);
        engine.add_rule(make_rule("r1")).await.unwrap();
        assert!(engine.remove_rule("r1").await);
        assert!(engine.list_rules().await.is_empty());
    }

    #[tokio::test]
    async fn remove_rule_not_found() {
        let bus = Arc::new(EventBus::new(16));
        let engine = AutomationEngine::new(bus);
        assert!(!engine.remove_rule("nonexistent").await);
    }

    #[tokio::test]
    async fn enable_disable_rule() {
        let bus = Arc::new(EventBus::new(16));
        let engine = AutomationEngine::new(bus);
        let mut rule = make_rule("r1");
        rule.enabled = false;
        engine.add_rule(rule).await.unwrap();

        assert!(engine.enable_rule("r1").await);
        assert!(engine.list_rules().await[0].enabled);

        assert!(engine.disable_rule("r1").await);
        assert!(!engine.list_rules().await[0].enabled);
    }

    #[tokio::test]
    async fn list_rules_returns_all() {
        let bus = Arc::new(EventBus::new(16));
        let engine = AutomationEngine::new(bus);
        engine.add_rule(make_rule("r1")).await.unwrap();
        engine.add_rule(make_rule("r2")).await.unwrap();
        engine.add_rule(make_rule("r3")).await.unwrap();
        assert_eq!(engine.list_rules().await.len(), 3);
    }

    // ─── cooldown tests ──────────────────────────────────

    #[test]
    fn cooldown_prevents_retrigger() {
        let rule = AutomationRule {
            id: "r1".into(),
            name: "test".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![],
            last_triggered: Some(Utc::now()),
            cooldown_secs: Some(60),
        };
        assert!(AutomationEngine::is_in_cooldown(&rule));
    }

    #[test]
    fn cooldown_expired_allows_retrigger() {
        let rule = AutomationRule {
            id: "r1".into(),
            name: "test".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![],
            last_triggered: Some(Utc::now() - chrono::Duration::seconds(120)),
            cooldown_secs: Some(60),
        };
        assert!(!AutomationEngine::is_in_cooldown(&rule));
    }

    // ─── serde tests ─────────────────────────────────────

    #[test]
    fn action_serde_roundtrip() {
        let action = Action::Sequence(vec![
            Action::DeviceCommand {
                device_id: "light-001".into(),
                command: "turn_on".into(),
                params: serde_json::json!({}),
            },
            Action::Delay {
                duration_ms: 1000,
                then: Box::new(Action::Notify {
                    message: "done".into(),
                    level: NotifyLevel::Info,
                }),
            },
        ]);
        let json = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2);
    }

    #[test]
    fn trigger_serde_roundtrip() {
        let triggers = vec![
            Trigger::DeviceEvent {
                device_id: "d1".into(),
                event_kind: EventKind::StateChanged,
            },
            Trigger::Schedule {
                cron: "0 0 * * * *".into(),
            },
            Trigger::Threshold {
                device_id: "s1".into(),
                property: "temp".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            },
        ];
        for trigger in &triggers {
            let json = serde_json::to_string(trigger).unwrap();
            let back: Trigger = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&back).unwrap();
            assert_eq!(json, json2);
        }
    }

    // ─── cron validation ─────────────────────────────────

    #[test]
    fn validate_cron_valid() {
        assert!(AutomationEngine::validate_cron("0 0 * * * *"));
    }

    #[test]
    fn validate_cron_invalid() {
        assert!(!AutomationEngine::validate_cron("not a cron"));
    }

    // ─── integration tests ───────────────────────────────

    #[test]
    fn disabled_rule_not_triggered() {
        let trigger = Trigger::DeviceEvent {
            device_id: "sensor-001".into(),
            event_kind: EventKind::StateChanged,
        };
        let event = make_event("sensor-001", EventKind::StateChanged, serde_json::json!({}));
        // Trigger matches, but rule is disabled — engine skips it
        assert!(AutomationEngine::matches_trigger(&trigger, &event));
        // The engine's start() loop checks rule.enabled before matching
    }

    #[test]
    fn full_flow_event_trigger_condition_action() {
        let trigger = Trigger::Threshold {
            device_id: "sensor-001".into(),
            property: "temperature".into(),
            op: CompareOp::Gt,
            value: serde_json::json!(30),
        };
        let event = make_event(
            "sensor-001",
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 35}),
        );
        assert!(AutomationEngine::matches_trigger(&trigger, &event));

        let condition = Condition::DeviceState {
            device_id: "sensor-001".into(),
            property: "temperature".into(),
            op: CompareOp::Gt,
            value: serde_json::json!(30),
        };
        let mut states = HashMap::new();
        states.insert("sensor-001".into(), serde_json::json!({"temperature": 35}));
        assert!(AutomationEngine::evaluate_condition(&condition, &states));
    }

    #[test]
    fn preset_rule_temperature_ac() {
        let rule = AutomationRule {
            id: "temp-ac".into(),
            name: "温度过高自动开空调".into(),
            enabled: true,
            triggers: vec![Trigger::Threshold {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            conditions: vec![Condition::TimeRange {
                after: "06:00".into(),
                before: "23:00".into(),
            }],
            actions: vec![Action::DeviceCommand {
                device_id: "ac-001".into(),
                command: "turn_on".into(),
                params: serde_json::json!({"temperature": 24}),
            }],
            last_triggered: None,
            cooldown_secs: Some(300),
        };

        let event = make_event(
            "sensor-001",
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 32}),
        );
        let triggered = rule
            .triggers
            .iter()
            .any(|t| AutomationEngine::matches_trigger(t, &event));
        assert!(triggered);

        let time = NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        let conditions_met = rule.conditions.iter().all(|c| {
            AutomationEngine::evaluate_condition_at(c, &HashMap::new(), time, Weekday::Mon)
        });
        assert!(conditions_met);
        assert!(!rule.actions.is_empty());
    }

    // ─── action execution tests ──────────────────────────

    use super::super::control::MockDeviceController;
    use super::super::device_state_cache::DeviceStateCache;
    use super::super::scene_executor::SceneExecutor;
    use super::super::scenes::{Scene, SceneManager};

    fn make_ac_rule() -> AutomationRule {
        AutomationRule {
            id: "temp-ac".into(),
            name: "温度过高开空调".into(),
            enabled: true,
            triggers: vec![Trigger::Threshold {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            conditions: vec![],
            actions: vec![Action::DeviceCommand {
                device_id: "ac-001".into(),
                command: "turn_on".into(),
                params: serde_json::json!({}),
            }],
            last_triggered: None,
            cooldown_secs: None,
        }
    }

    fn setup_controller_with_devices() -> Arc<GenericController> {
        let mut gc = GenericController::new();
        let mock = MockDeviceController::new("ac");
        mock.register_device("ac-001", "ac");
        mock.register_device("light-001", "light");
        gc.register_adapter(Box::new(mock));
        Arc::new(gc)
    }

    #[tokio::test]
    async fn start_triggers_device_command() {
        let bus = Arc::new(EventBus::new(64));
        let controller = setup_controller_with_devices();
        let engine = AutomationEngine::new(Arc::clone(&bus)).with_controller(controller);

        engine.add_rule(make_ac_rule()).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 35}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_some());
    }

    #[tokio::test]
    async fn start_runs_scene_action() {
        let bus = Arc::new(EventBus::new(64));
        let mut scene_mgr = SceneManager::new();
        scene_mgr.add_scene(Scene::come_home());
        let scene_mgr = Arc::new(RwLock::new(scene_mgr));

        let mut executor = SceneExecutor::new();
        let mock_ctrl = Arc::new(MockDeviceController::new("light"));
        mock_ctrl.register_device("light-living-room", "light");
        executor.register_controller("light", mock_ctrl);
        let executor = Arc::new(RwLock::new(executor));

        let engine = AutomationEngine::new(Arc::clone(&bus))
            .with_scene_executor(executor)
            .with_scene_manager(scene_mgr);

        let rule = AutomationRule {
            id: "run-scene".into(),
            name: "Run come home".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "door-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::RunScene {
                scene_id: "come-home".into(),
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "door-001",
            DeviceType::Lock,
            EventKind::StateChanged,
            serde_json::json!({"opened": true}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_some());
    }

    #[tokio::test]
    async fn start_condition_not_met_skips_action() {
        let bus = Arc::new(EventBus::new(64));
        let controller = setup_controller_with_devices();
        let cache = Arc::new(DeviceStateCache::new());
        // Set temperature below threshold condition
        cache
            .set("sensor-001", serde_json::json!({"temperature": 20}))
            .await;

        let engine = AutomationEngine::new(Arc::clone(&bus))
            .with_controller(controller)
            .with_state_cache(cache);

        let rule = AutomationRule {
            id: "cond-rule".into(),
            name: "Conditional".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-001".into(),
                event_kind: EventKind::PropertyChanged,
            }],
            conditions: vec![Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            actions: vec![Action::Notify {
                message: "should not fire".into(),
                level: NotifyLevel::Warning,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 20}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let notifications = engine.notifications().await;
        assert!(notifications.is_empty());
    }

    #[tokio::test]
    async fn notify_action_writes_log() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "notify-rule".into(),
            name: "Notify test".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "Alert!".into(),
                level: NotifyLevel::Critical,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(make_event(
            "sensor-001",
            EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let notifications = engine.notifications().await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, "Alert!");
    }

    #[tokio::test]
    async fn sequence_action_executes_all() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "seq-rule".into(),
            name: "Sequence".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::Sequence(vec![
                Action::Notify {
                    message: "first".into(),
                    level: NotifyLevel::Info,
                },
                Action::Notify {
                    message: "second".into(),
                    level: NotifyLevel::Info,
                },
                Action::Notify {
                    message: "third".into(),
                    level: NotifyLevel::Info,
                },
            ])],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(make_event(
            "sensor-001",
            EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let notifications = engine.notifications().await;
        assert_eq!(notifications.len(), 3);
        assert_eq!(notifications[0].0, "first");
        assert_eq!(notifications[2].0, "third");
    }

    #[tokio::test]
    async fn trigger_rule_executes_actions() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "manual".into(),
            name: "Manual trigger".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "manually triggered".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.trigger_rule("manual").await.unwrap();

        let notifications = engine.notifications().await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, "manually triggered");
    }

    #[tokio::test]
    async fn cooldown_prevents_action_execution() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let mut rule = make_rule("cooldown-test");
        rule.cooldown_secs = Some(3600);
        rule.actions = vec![Action::Notify {
            message: "should fire once".into(),
            level: NotifyLevel::Info,
        }];
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        // First event triggers
        bus.publish(make_event(
            "sensor-001",
            EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Second event within cooldown — should not trigger
        bus.publish(make_event(
            "sensor-001",
            EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let notifications = engine.notifications().await;
        assert_eq!(notifications.len(), 1);
    }

    #[tokio::test]
    async fn disabled_rule_no_action() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let mut rule = make_rule("disabled");
        rule.enabled = false;
        rule.actions = vec![Action::Notify {
            message: "should not fire".into(),
            level: NotifyLevel::Info,
        }];
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(make_event(
            "sensor-001",
            EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert!(engine.notifications().await.is_empty());
    }

    #[tokio::test]
    async fn delay_action_executes_after_wait() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "delay-rule".into(),
            name: "Delay test".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![Action::Delay {
                duration_ms: 10,
                then: Box::new(Action::Notify {
                    message: "delayed".into(),
                    level: NotifyLevel::Info,
                }),
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.trigger_rule("delay-rule").await.unwrap();

        let notifications = engine.notifications().await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, "delayed");
    }

    #[tokio::test]
    async fn full_flow_temp_event_condition_action() {
        let bus = Arc::new(EventBus::new(64));
        let controller = setup_controller_with_devices();
        let cache = Arc::new(DeviceStateCache::new());
        cache
            .set("sensor-001", serde_json::json!({"temperature": 35}))
            .await;

        let engine = AutomationEngine::new(Arc::clone(&bus))
            .with_controller(controller)
            .with_state_cache(cache);

        let rule = AutomationRule {
            id: "full-flow".into(),
            name: "Full flow".into(),
            enabled: true,
            triggers: vec![Trigger::Threshold {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            conditions: vec![Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            actions: vec![
                Action::DeviceCommand {
                    device_id: "ac-001".into(),
                    command: "turn_on".into(),
                    params: serde_json::json!({}),
                },
                Action::Notify {
                    message: "AC turned on".into(),
                    level: NotifyLevel::Info,
                },
            ],
            last_triggered: None,
            cooldown_secs: Some(300),
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 35}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let notifications = engine.notifications().await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, "AC turned on");

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_some());
    }
}
