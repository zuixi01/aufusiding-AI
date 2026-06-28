//! 系统模式管理器
//!
//! 管理 Home/Work/Sleep/Driving 等系统模式，控制通知策略、场景激活、设备配置。
//! 支持自动检测（地理围栏、时间范围、车辆连接）和手动覆盖。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

// ─── 核心枚举 ───────────────────────────────────────────

/// 系统模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemMode {
    Home,
    Work,
    Sleep,
    Driving,
    Away,
    Cinema,
    Focus,
    Guest,
}

/// 通知策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPolicy {
    pub allow_critical: bool,
    pub allow_high: bool,
    pub allow_normal: bool,
    pub allow_low: bool,
    pub silent: bool,
}

impl NotificationPolicy {
    pub fn allow_all() -> Self {
        Self { allow_critical: true, allow_high: true, allow_normal: true, allow_low: true, silent: false }
    }

    pub fn critical_only() -> Self {
        Self { allow_critical: true, allow_high: false, allow_normal: false, allow_low: false, silent: true }
    }

    pub fn silent() -> Self {
        Self { allow_critical: true, allow_high: false, allow_normal: false, allow_low: false, silent: true }
    }
}

/// 模式配置文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeProfile {
    pub mode: SystemMode,
    pub notification_policy: NotificationPolicy,
    pub active_scenes: Vec<String>,
    pub deactivate_scenes: Vec<String>,
    pub device_configs: HashMap<String, serde_json::Value>,
    pub volume_level: u8,
}

/// 自动检测来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoDetectSource {
    Geofence { zone_id: String },
    TimeRange { start: NaiveTime, end: NaiveTime },
    VehicleConnection { device_id: String },
    Manual,
}

/// 自动切换规则
#[derive(Debug, Clone)]
pub struct AutoRule {
    pub source: AutoDetectSource,
    pub target_mode: SystemMode,
    pub priority: u8,
}

/// 模式切换记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeTransition {
    pub from: SystemMode,
    pub to: SystemMode,
    pub timestamp: DateTime<Utc>,
    pub source: AutoDetectSource,
}

/// 自动检测上下文
#[derive(Debug, Clone, Default)]
pub struct DetectContext {
    pub current_zone: Option<String>,
    pub current_time: Option<NaiveTime>,
    pub connected_vehicle: Option<String>,
}

// ─── Hook trait ──────────────────────────────────────────

/// 模式切换钩子
#[async_trait]
pub trait ModeTransitionHook: Send + Sync {
    async fn on_enter(&self, mode: SystemMode) -> anyhow::Result<()>;
    async fn on_exit(&self, mode: SystemMode) -> anyhow::Result<()>;
}

/// 测试用 Mock 模式切换钩子
pub struct MockModeTransitionHook {
    entered: Mutex<Vec<SystemMode>>,
    exited: Mutex<Vec<SystemMode>>,
}

impl MockModeTransitionHook {
    pub fn new() -> Self {
        Self {
            entered: Mutex::new(Vec::new()),
            exited: Mutex::new(Vec::new()),
        }
    }

    pub fn entered_modes(&self) -> Vec<SystemMode> {
        self.entered.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn exited_modes(&self) -> Vec<SystemMode> {
        self.exited.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[async_trait]
impl ModeTransitionHook for MockModeTransitionHook {
    async fn on_enter(&self, mode: SystemMode) -> anyhow::Result<()> {
        self.entered.lock().unwrap_or_else(|e| e.into_inner()).push(mode);
        Ok(())
    }
    async fn on_exit(&self, mode: SystemMode) -> anyhow::Result<()> {
        self.exited.lock().unwrap_or_else(|e| e.into_inner()).push(mode);
        Ok(())
    }
}

// ─── ModeManager ─────────────────────────────────────────

/// 系统模式管理器
pub struct ModeManager {
    current_mode: Mutex<SystemMode>,
    profiles: HashMap<SystemMode, ModeProfile>,
    auto_rules: Vec<AutoRule>,
    history: Mutex<Vec<ModeTransition>>,
    hooks: Vec<Box<dyn ModeTransitionHook>>,
    manual_override: Mutex<bool>,
}

impl ModeManager {
    pub fn new(default_mode: SystemMode) -> Self {
        let mut profiles = HashMap::new();
        for (mode, profile) in Self::default_profiles() {
            profiles.insert(mode, profile);
        }
        Self {
            current_mode: Mutex::new(default_mode),
            profiles,
            auto_rules: Vec::new(),
            history: Mutex::new(Vec::new()),
            hooks: Vec::new(),
            manual_override: Mutex::new(false),
        }
    }

    pub fn register_profile(&mut self, profile: ModeProfile) {
        self.profiles.insert(profile.mode, profile);
    }

    pub fn register_hook(&mut self, hook: Box<dyn ModeTransitionHook>) {
        self.hooks.push(hook);
    }

    pub fn add_auto_rule(&mut self, rule: AutoRule) {
        self.auto_rules.push(rule);
    }

    pub fn current_mode(&self) -> SystemMode {
        *self.current_mode.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_manual_override(&self) -> bool {
        *self.manual_override.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub async fn switch_mode(
        &self,
        target: SystemMode,
        source: AutoDetectSource,
    ) -> anyhow::Result<()> {
        let from = {
            let current = self.current_mode.lock().unwrap_or_else(|e| e.into_inner());
            if *current == target {
                return Ok(());
            }
            *current
        };

        // on_exit hooks
        for hook in &self.hooks {
            hook.on_exit(from).await?;
        }

        // update current mode
        {
            let mut current = self.current_mode.lock().unwrap_or_else(|e| e.into_inner());
            *current = target;
        }

        // set manual override flag
        {
            let mut mo = self.manual_override.lock().unwrap_or_else(|e| e.into_inner());
            *mo = matches!(source, AutoDetectSource::Manual);
        }

        // on_enter hooks
        for hook in &self.hooks {
            hook.on_enter(target).await?;
        }

        // record history
        {
            let mut history = self.history.lock().unwrap_or_else(|e| e.into_inner());
            history.push(ModeTransition {
                from,
                to: target,
                timestamp: Utc::now(),
                source,
            });
        }

        Ok(())
    }

    pub fn evaluate_auto_switch(&self, context: &DetectContext) -> Option<SystemMode> {
        if self.is_manual_override() {
            return None;
        }
        let mut best: Option<&AutoRule> = None;
        for rule in &self.auto_rules {
            let matched = match &rule.source {
                AutoDetectSource::Geofence { zone_id } => {
                    context.current_zone.as_deref() == Some(zone_id.as_str())
                }
                AutoDetectSource::TimeRange { start, end } => {
                    if let Some(now) = context.current_time {
                        if start <= end { now >= *start && now < *end }
                        else { now >= *start || now < *end }
                    } else { false }
                }
                AutoDetectSource::VehicleConnection { device_id } => {
                    context.connected_vehicle.as_deref() == Some(device_id.as_str())
                }
                AutoDetectSource::Manual => false,
            };
            if matched && best.is_none_or(|b| rule.priority > b.priority) {
                best = Some(rule);
            }
        }
        let current = self.current_mode();
        best.filter(|r| r.target_mode != current).map(|r| r.target_mode)
    }

    pub fn notification_policy(&self) -> NotificationPolicy {
        let mode = self.current_mode();
        self.profiles
            .get(&mode)
            .map(|p| p.notification_policy.clone())
            .unwrap_or_else(NotificationPolicy::allow_all)
    }

    pub fn get_profile(&self, mode: &SystemMode) -> Option<&ModeProfile> {
        self.profiles.get(mode)
    }

    pub fn history(&self) -> Vec<ModeTransition> {
        self.history.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn default_profiles() -> Vec<(SystemMode, ModeProfile)> {
        vec![
            (SystemMode::Home, ModeProfile {
                mode: SystemMode::Home,
                notification_policy: NotificationPolicy::allow_all(),
                active_scenes: vec!["welcome_home".into()],
                deactivate_scenes: vec![],
                device_configs: HashMap::new(),
                volume_level: 60,
            }),
            (SystemMode::Work, ModeProfile {
                mode: SystemMode::Work,
                notification_policy: NotificationPolicy {
                    allow_critical: true, allow_high: true,
                    allow_normal: false, allow_low: false, silent: false,
                },
                active_scenes: vec!["work_focus".into()],
                deactivate_scenes: vec!["welcome_home".into()],
                device_configs: HashMap::new(),
                volume_level: 30,
            }),
            (SystemMode::Sleep, ModeProfile {
                mode: SystemMode::Sleep,
                notification_policy: NotificationPolicy::critical_only(),
                active_scenes: vec!["good_night".into()],
                deactivate_scenes: vec!["welcome_home".into(), "work_focus".into()],
                device_configs: HashMap::new(),
                volume_level: 10,
            }),
            (SystemMode::Driving, ModeProfile {
                mode: SystemMode::Driving,
                notification_policy: NotificationPolicy {
                    allow_critical: true, allow_high: true,
                    allow_normal: false, allow_low: false, silent: false,
                },
                active_scenes: vec!["driving".into()],
                deactivate_scenes: vec![],
                device_configs: HashMap::new(),
                volume_level: 50,
            }),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHook {
        entered: Mutex<Vec<SystemMode>>,
        exited: Mutex<Vec<SystemMode>>,
    }

    impl TestHook {
        fn new() -> Self {
            Self { entered: Mutex::new(Vec::new()), exited: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl ModeTransitionHook for TestHook {
        async fn on_enter(&self, mode: SystemMode) -> anyhow::Result<()> {
            self.entered.lock().unwrap().push(mode);
            Ok(())
        }
        async fn on_exit(&self, mode: SystemMode) -> anyhow::Result<()> {
            self.exited.lock().unwrap().push(mode);
            Ok(())
        }
    }

    struct HookWrapper(std::sync::Arc<TestHook>);

    #[async_trait]
    impl ModeTransitionHook for HookWrapper {
        async fn on_enter(&self, mode: SystemMode) -> anyhow::Result<()> {
            self.0.on_enter(mode).await
        }
        async fn on_exit(&self, mode: SystemMode) -> anyhow::Result<()> {
            self.0.on_exit(mode).await
        }
    }

    #[test]
    fn new_default_mode() {
        let mgr = ModeManager::new(SystemMode::Home);
        assert_eq!(mgr.current_mode(), SystemMode::Home);
    }

    #[test]
    fn default_profiles_loaded() {
        let mgr = ModeManager::new(SystemMode::Home);
        assert!(mgr.get_profile(&SystemMode::Home).is_some());
        assert!(mgr.get_profile(&SystemMode::Work).is_some());
        assert!(mgr.get_profile(&SystemMode::Sleep).is_some());
        assert!(mgr.get_profile(&SystemMode::Driving).is_some());
    }

    #[tokio::test]
    async fn switch_mode_updates_current() {
        let mgr = ModeManager::new(SystemMode::Home);
        mgr.switch_mode(SystemMode::Work, AutoDetectSource::Manual).await.unwrap();
        assert_eq!(mgr.current_mode(), SystemMode::Work);
    }

    #[tokio::test]
    async fn switch_same_mode_noop() {
        let mgr = ModeManager::new(SystemMode::Home);
        mgr.switch_mode(SystemMode::Home, AutoDetectSource::Manual).await.unwrap();
        assert!(mgr.history().is_empty());
    }

    #[tokio::test]
    async fn switch_mode_records_history() {
        let mgr = ModeManager::new(SystemMode::Home);
        mgr.switch_mode(SystemMode::Sleep, AutoDetectSource::Geofence { zone_id: "home".into() })
            .await.unwrap();
        let hist = mgr.history();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].from, SystemMode::Home);
        assert_eq!(hist[0].to, SystemMode::Sleep);
    }

    #[tokio::test]
    async fn manual_override_flag() {
        let mgr = ModeManager::new(SystemMode::Home);
        assert!(!mgr.is_manual_override());
        mgr.switch_mode(SystemMode::Focus, AutoDetectSource::Manual).await.unwrap();
        assert!(mgr.is_manual_override());
    }

    #[tokio::test]
    async fn hook_called_on_switch() {
        let hook = std::sync::Arc::new(TestHook::new());
        let hook_clone: Box<dyn ModeTransitionHook> = Box::new(HookWrapper(std::sync::Arc::clone(&hook)));
        let mut mgr = ModeManager::new(SystemMode::Home);
        mgr.register_hook(hook_clone);
        mgr.switch_mode(SystemMode::Work, AutoDetectSource::Manual).await.unwrap();
        assert_eq!(hook.exited.lock().unwrap().as_slice(), &[SystemMode::Home]);
        assert_eq!(hook.entered.lock().unwrap().as_slice(), &[SystemMode::Work]);
    }

    #[test]
    fn evaluate_auto_switch_geofence() {
        let mut mgr = ModeManager::new(SystemMode::Home);
        mgr.add_auto_rule(AutoRule {
            source: AutoDetectSource::Geofence { zone_id: "office".into() },
            target_mode: SystemMode::Work,
            priority: 10,
        });
        let ctx = DetectContext { current_zone: Some("office".into()), ..Default::default() };
        assert_eq!(mgr.evaluate_auto_switch(&ctx), Some(SystemMode::Work));
    }

    #[test]
    fn evaluate_auto_switch_time_range() {
        let mut mgr = ModeManager::new(SystemMode::Home);
        mgr.add_auto_rule(AutoRule {
            source: AutoDetectSource::TimeRange {
                start: NaiveTime::from_hms_opt(23, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(7, 0, 0).unwrap(),
            },
            target_mode: SystemMode::Sleep,
            priority: 5,
        });
        let ctx = DetectContext {
            current_time: Some(NaiveTime::from_hms_opt(1, 0, 0).unwrap()),
            ..Default::default()
        };
        assert_eq!(mgr.evaluate_auto_switch(&ctx), Some(SystemMode::Sleep));
    }

    #[test]
    fn evaluate_auto_switch_vehicle() {
        let mut mgr = ModeManager::new(SystemMode::Home);
        mgr.add_auto_rule(AutoRule {
            source: AutoDetectSource::VehicleConnection { device_id: "tesla-001".into() },
            target_mode: SystemMode::Driving,
            priority: 20,
        });
        let ctx = DetectContext { connected_vehicle: Some("tesla-001".into()), ..Default::default() };
        assert_eq!(mgr.evaluate_auto_switch(&ctx), Some(SystemMode::Driving));
    }

    #[tokio::test]
    async fn manual_override_blocks_auto_switch() {
        let mgr = ModeManager::new(SystemMode::Home);
        mgr.switch_mode(SystemMode::Focus, AutoDetectSource::Manual).await.unwrap();
        let ctx = DetectContext { current_zone: Some("office".into()), ..Default::default() };
        assert_eq!(mgr.evaluate_auto_switch(&ctx), None);
    }

    #[test]
    fn evaluate_no_match_returns_none() {
        let mgr = ModeManager::new(SystemMode::Home);
        let ctx = DetectContext::default();
        assert_eq!(mgr.evaluate_auto_switch(&ctx), None);
    }

    #[test]
    fn evaluate_same_mode_returns_none() {
        let mut mgr = ModeManager::new(SystemMode::Work);
        mgr.add_auto_rule(AutoRule {
            source: AutoDetectSource::Geofence { zone_id: "office".into() },
            target_mode: SystemMode::Work,
            priority: 10,
        });
        let ctx = DetectContext { current_zone: Some("office".into()), ..Default::default() };
        assert_eq!(mgr.evaluate_auto_switch(&ctx), None);
    }

    #[test]
    fn evaluate_priority_wins() {
        let mut mgr = ModeManager::new(SystemMode::Home);
        mgr.add_auto_rule(AutoRule {
            source: AutoDetectSource::Geofence { zone_id: "office".into() },
            target_mode: SystemMode::Work,
            priority: 5,
        });
        mgr.add_auto_rule(AutoRule {
            source: AutoDetectSource::Geofence { zone_id: "office".into() },
            target_mode: SystemMode::Focus,
            priority: 15,
        });
        let ctx = DetectContext { current_zone: Some("office".into()), ..Default::default() };
        assert_eq!(mgr.evaluate_auto_switch(&ctx), Some(SystemMode::Focus));
    }

    #[test]
    fn notification_policy_for_mode() {
        let mgr = ModeManager::new(SystemMode::Sleep);
        let policy = mgr.notification_policy();
        assert!(policy.allow_critical);
        assert!(!policy.allow_normal);
    }

    #[test]
    fn register_custom_profile() {
        let mut mgr = ModeManager::new(SystemMode::Home);
        mgr.register_profile(ModeProfile {
            mode: SystemMode::Cinema,
            notification_policy: NotificationPolicy::silent(),
            active_scenes: vec!["movie_time".into()],
            deactivate_scenes: vec![],
            device_configs: HashMap::new(),
            volume_level: 80,
        });
        let p = mgr.get_profile(&SystemMode::Cinema).unwrap();
        assert_eq!(p.volume_level, 80);
        assert!(p.notification_policy.silent);
    }

    #[test]
    fn system_mode_serde_roundtrip() {
        let mode = SystemMode::Driving;
        let json = serde_json::to_string(&mode).unwrap();
        let back: SystemMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }

    #[test]
    fn mode_transition_serde_roundtrip() {
        let t = ModeTransition {
            from: SystemMode::Home,
            to: SystemMode::Work,
            timestamp: Utc::now(),
            source: AutoDetectSource::Geofence { zone_id: "office".into() },
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: ModeTransition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.from, SystemMode::Home);
        assert_eq!(back.to, SystemMode::Work);
    }
}
