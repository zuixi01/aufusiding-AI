//! 主动设备推理器
//!
//! 评估上下文信号（时间、房间占用、设备状态、室外温度等），
//! 生成节能、舒适、安全、便利类别的设备操作建议。

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::profile::SemanticProfileStore;
use crate::iot::scenes::SceneAction;
use crate::iot::types::DeviceType;

/// 上下文信号
#[derive(Debug, Clone)]
pub struct DeviceContextSignal {
    pub time_of_day: NaiveTime,
    pub day_of_week: Weekday,
    pub room_occupancy: HashMap<String, bool>,
    pub device_states: HashMap<String, Value>,
    pub outdoor_temperature: Option<f64>,
}

/// 建议类别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionCategory {
    /// 舒适度
    Comfort,
    /// 节能
    Energy,
    /// 安全
    Security,
    /// 便利
    Convenience,
}

/// 主动建议
#[derive(Debug, Clone)]
pub struct ProactiveSuggestion {
    pub description: String,
    pub actions: Vec<SceneAction>,
    pub confidence: f64,
    pub reason: String,
    pub category: SuggestionCategory,
}

/// 主动设备推理器
pub struct ProactiveDeviceReasoner {
    store: Arc<SemanticProfileStore>,
}
// ─── 辅助函数 ────────────────────────────────────────────

fn is_device_powered(device_states: &HashMap<String, Value>, device_id: &str) -> bool {
    device_states
        .get(device_id)
        .and_then(|v| v.get("powered"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn get_brightness(device_states: &HashMap<String, Value>, device_id: &str) -> Option<i64> {
    device_states
        .get(device_id)
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.get("brightness"))
        .and_then(|v| v.as_i64())
}

impl ProactiveDeviceReasoner {
    pub fn new(store: Arc<SemanticProfileStore>) -> Self {
        Self { store }
    }

    /// 评估上下文信号，生成主动建议
    pub fn evaluate(&self, signal: &DeviceContextSignal) -> Vec<ProactiveSuggestion> {
        let mut suggestions = Vec::new();
        self.check_empty_room_lights(signal, &mut suggestions);
        self.check_bedtime_dimming(signal, &mut suggestions);
        self.check_temperature_comfort(signal, &mut suggestions);
        self.check_security_lock(signal, &mut suggestions);
        self.check_morning_routine(signal, &mut suggestions);
        suggestions
    }

    fn check_empty_room_lights(
        &self,
        signal: &DeviceContextSignal,
        suggestions: &mut Vec<ProactiveSuggestion>,
    ) {
        for (room, &occupied) in &signal.room_occupancy {
            if occupied {
                continue;
            }
            let profiles = self.store.query_by_room(room);
            for p in profiles {
                if p.device_type != DeviceType::Light {
                    continue;
                }
                if !is_device_powered(&signal.device_states, &p.device_id) {
                    continue;
                }
                suggestions.push(ProactiveSuggestion {
                    description: format!("{}的{}已开启，但房间无人", room, p.friendly_name),
                    actions: vec![SceneAction {
                        device_id: p.device_id.clone(),
                        command: "turn_off".into(),
                        params: serde_json::json!({"device_type": "Light"}),
                        delay_ms: 0,
                    }],
                    confidence: 0.8,
                    reason: "节能：空房间灯光仍在开启".into(),
                    category: SuggestionCategory::Energy,
                });
            }
        }
    }
    fn check_bedtime_dimming(
        &self,
        signal: &DeviceContextSignal,
        suggestions: &mut Vec<ProactiveSuggestion>,
    ) {
        let bedtime = NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        if signal.time_of_day < bedtime {
            return;
        }
        for p in self.store.list_all() {
            if p.device_type != DeviceType::Light {
                continue;
            }
            if !is_device_powered(&signal.device_states, &p.device_id) {
                continue;
            }
            if let Some(brightness) = get_brightness(&signal.device_states, &p.device_id) {
                if brightness > 50 {
                    suggestions.push(ProactiveSuggestion {
                        description: format!("建议调暗{}", p.friendly_name),
                        actions: vec![SceneAction {
                            device_id: p.device_id.clone(),
                            command: "set_property".into(),
                            params: serde_json::json!({
                                "property": "brightness",
                                "value": 30,
                                "device_type": "Light"
                            }),
                            delay_ms: 0,
                        }],
                        confidence: 0.7,
                        reason: "夜间舒适：建议降低灯光亮度".into(),
                        category: SuggestionCategory::Comfort,
                    });
                }
            }
        }
    }

    fn check_temperature_comfort(
        &self,
        signal: &DeviceContextSignal,
        suggestions: &mut Vec<ProactiveSuggestion>,
    ) {
        let outdoor_temp = match signal.outdoor_temperature {
            Some(t) if t > 28.0 => t,
            _ => return,
        };
        for p in self.store.list_all() {
            if p.device_type != DeviceType::Thermostat {
                continue;
            }
            if is_device_powered(&signal.device_states, &p.device_id) {
                continue;
            }
            suggestions.push(ProactiveSuggestion {
                description: format!("室外温度较高，建议开启{}", p.friendly_name),
                actions: vec![
                    SceneAction {
                        device_id: p.device_id.clone(),
                        command: "turn_on".into(),
                        params: serde_json::json!({"device_type": "Thermostat"}),
                        delay_ms: 0,
                    },
                    SceneAction {
                        device_id: p.device_id.clone(),
                        command: "set_property".into(),
                        params: serde_json::json!({
                            "property": "temperature",
                            "value": 25,
                            "device_type": "Thermostat"
                        }),
                        delay_ms: 500,
                    },
                ],
                confidence: 0.75,
                reason: format!("室外温度{}°C，建议开启空调", outdoor_temp),
                category: SuggestionCategory::Comfort,
            });
        }
    }
    fn check_security_lock(
        &self,
        signal: &DeviceContextSignal,
        suggestions: &mut Vec<ProactiveSuggestion>,
    ) {
        let late_night = NaiveTime::from_hms_opt(23, 0, 0).unwrap();
        if signal.time_of_day < late_night {
            return;
        }
        for p in self.store.list_all() {
            if p.device_type != DeviceType::Lock {
                continue;
            }
            if is_device_powered(&signal.device_states, &p.device_id) {
                continue;
            }
            suggestions.push(ProactiveSuggestion {
                description: format!("建议锁定{}", p.friendly_name),
                actions: vec![SceneAction {
                    device_id: p.device_id.clone(),
                    command: "turn_on".into(),
                    params: serde_json::json!({"device_type": "Lock"}),
                    delay_ms: 0,
                }],
                confidence: 0.9,
                reason: "夜间安全：门锁未锁定".into(),
                category: SuggestionCategory::Security,
            });
        }
    }

    fn check_morning_routine(
        &self,
        signal: &DeviceContextSignal,
        suggestions: &mut Vec<ProactiveSuggestion>,
    ) {
        let morning_start = NaiveTime::from_hms_opt(6, 0, 0).unwrap();
        let morning_end = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        if signal.time_of_day < morning_start || signal.time_of_day > morning_end {
            return;
        }
        let is_weekday = matches!(
            signal.day_of_week,
            Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
        );
        if !is_weekday {
            return;
        }
        // Find lights in occupied rooms, or all lights if no occupancy data
        let occupied_rooms: Vec<&String> = signal
            .room_occupancy
            .iter()
            .filter(|(_, &occ)| occ)
            .map(|(room, _)| room)
            .collect();

        for p in self.store.list_all() {
            if p.device_type != DeviceType::Light {
                continue;
            }
            if is_device_powered(&signal.device_states, &p.device_id) {
                continue;
            }
            // Only suggest for occupied rooms (or any room if no occupancy data)
            if !occupied_rooms.is_empty() {
                if let Some(ref room) = p.room {
                    if !occupied_rooms.contains(&room) {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            suggestions.push(ProactiveSuggestion {
                description: format!("早安，建议开启{}", p.friendly_name),
                actions: vec![SceneAction {
                    device_id: p.device_id.clone(),
                    command: "turn_on".into(),
                    params: serde_json::json!({"device_type": "Light"}),
                    delay_ms: 0,
                }],
                confidence: 0.6,
                reason: "工作日晨间：建议开启照明".into(),
                category: SuggestionCategory::Convenience,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::profile::build_default_profile;
    use super::*;

    fn build_test_store() -> Arc<SemanticProfileStore> {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile(
            "light-001",
            "客厅灯",
            DeviceType::Light,
            Some("客厅"),
        ));
        store.add(build_default_profile(
            "light-002",
            "卧室灯",
            DeviceType::Light,
            Some("卧室"),
        ));
        store.add(build_default_profile(
            "thermo-001",
            "客厅空调",
            DeviceType::Thermostat,
            Some("客厅"),
        ));
        store.add(build_default_profile(
            "lock-001",
            "前门锁",
            DeviceType::Lock,
            None,
        ));
        Arc::new(store)
    }

    fn base_signal() -> DeviceContextSignal {
        DeviceContextSignal {
            time_of_day: NaiveTime::from_hms_opt(14, 0, 0).unwrap(),
            day_of_week: Weekday::Wed,
            room_occupancy: HashMap::new(),
            device_states: HashMap::new(),
            outdoor_temperature: None,
        }
    }

    fn powered_on() -> Value {
        serde_json::json!({"powered": true})
    }

    fn powered_off() -> Value {
        serde_json::json!({"powered": false})
    }

    fn powered_on_with_brightness(b: i64) -> Value {
        serde_json::json!({"powered": true, "properties": {"brightness": b}})
    }

    #[test]
    fn empty_room_light_on_suggests_off() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.room_occupancy.insert("客厅".into(), false);
        signal
            .device_states
            .insert("light-001".into(), powered_on());
        let suggestions = reasoner.evaluate(&signal);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].category, SuggestionCategory::Energy);
        assert!(suggestions[0].description.contains("客厅"));
        assert_eq!(suggestions[0].actions[0].command, "turn_off");
    }

    #[test]
    fn empty_room_light_off_no_suggestion() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.room_occupancy.insert("客厅".into(), false);
        signal
            .device_states
            .insert("light-001".into(), powered_off());
        let suggestions = reasoner.evaluate(&signal);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn occupied_room_light_on_no_suggestion() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.room_occupancy.insert("客厅".into(), true);
        signal
            .device_states
            .insert("light-001".into(), powered_on());
        let suggestions = reasoner.evaluate(&signal);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn bedtime_bright_light_suggests_dim() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        signal
            .device_states
            .insert("light-001".into(), powered_on_with_brightness(80));
        let suggestions = reasoner.evaluate(&signal);
        let dim: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Comfort)
            .collect();
        assert!(!dim.is_empty());
        assert_eq!(dim[0].actions[0].command, "set_property");
    }

    #[test]
    fn bedtime_dim_light_no_suggestion() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        signal
            .device_states
            .insert("light-001".into(), powered_on_with_brightness(30));
        let suggestions = reasoner.evaluate(&signal);
        let dim: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Comfort)
            .collect();
        assert!(dim.is_empty());
    }

    #[test]
    fn daytime_bright_no_suggestion() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        signal
            .device_states
            .insert("light-001".into(), powered_on_with_brightness(80));
        let suggestions = reasoner.evaluate(&signal);
        let dim: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Comfort)
            .collect();
        assert!(dim.is_empty());
    }

    #[test]
    fn hot_weather_ac_off_suggests_on() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.outdoor_temperature = Some(32.0);
        signal
            .device_states
            .insert("thermo-001".into(), powered_off());
        let suggestions = reasoner.evaluate(&signal);
        let ac: Vec<_> = suggestions
            .iter()
            .filter(|s| s.description.contains("空调"))
            .collect();
        assert_eq!(ac.len(), 1);
        assert_eq!(ac[0].category, SuggestionCategory::Comfort);
        assert_eq!(ac[0].actions.len(), 2);
    }

    #[test]
    fn hot_weather_ac_on_no_suggestion() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.outdoor_temperature = Some(32.0);
        signal
            .device_states
            .insert("thermo-001".into(), powered_on());
        let suggestions = reasoner.evaluate(&signal);
        let ac: Vec<_> = suggestions
            .iter()
            .filter(|s| s.description.contains("空调"))
            .collect();
        assert!(ac.is_empty());
    }

    #[test]
    fn cool_weather_no_suggestion() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.outdoor_temperature = Some(20.0);
        signal
            .device_states
            .insert("thermo-001".into(), powered_off());
        let suggestions = reasoner.evaluate(&signal);
        let ac: Vec<_> = suggestions
            .iter()
            .filter(|s| s.description.contains("空调"))
            .collect();
        assert!(ac.is_empty());
    }

    #[test]
    fn late_night_unlocked_suggests_lock() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(23, 30, 0).unwrap();
        signal
            .device_states
            .insert("lock-001".into(), powered_off());
        let suggestions = reasoner.evaluate(&signal);
        let locks: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Security)
            .collect();
        assert_eq!(locks.len(), 1);
        assert!(locks[0].description.contains("前门锁"));
    }

    #[test]
    fn late_night_locked_no_suggestion() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(23, 30, 0).unwrap();
        signal.device_states.insert("lock-001".into(), powered_on());
        let suggestions = reasoner.evaluate(&signal);
        let locks: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Security)
            .collect();
        assert!(locks.is_empty());
    }

    #[test]
    fn morning_weekday_light_off_suggests_on() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(7, 0, 0).unwrap();
        signal.day_of_week = Weekday::Mon;
        signal.room_occupancy.insert("客厅".into(), true);
        signal
            .device_states
            .insert("light-001".into(), powered_off());
        let suggestions = reasoner.evaluate(&signal);
        let morning: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Convenience)
            .collect();
        assert_eq!(morning.len(), 1);
        assert!(morning[0].description.contains("早安"));
    }

    #[test]
    fn morning_weekend_no_suggestion() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(7, 0, 0).unwrap();
        signal.day_of_week = Weekday::Sat;
        signal.room_occupancy.insert("客厅".into(), true);
        signal
            .device_states
            .insert("light-001".into(), powered_off());
        let suggestions = reasoner.evaluate(&signal);
        let morning: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Convenience)
            .collect();
        assert!(morning.is_empty());
    }

    #[test]
    fn multiple_rules_trigger() {
        let store = build_test_store();
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(23, 30, 0).unwrap();
        signal
            .device_states
            .insert("light-001".into(), powered_on_with_brightness(80));
        signal
            .device_states
            .insert("lock-001".into(), powered_off());
        let suggestions = reasoner.evaluate(&signal);
        let comfort: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Comfort)
            .collect();
        let security: Vec<_> = suggestions
            .iter()
            .filter(|s| s.category == SuggestionCategory::Security)
            .collect();
        assert!(
            !comfort.is_empty(),
            "should have bedtime dimming suggestion"
        );
        assert!(!security.is_empty(), "should have lock suggestion");
    }

    #[test]
    fn no_devices_no_suggestions() {
        let store = Arc::new(SemanticProfileStore::new());
        let reasoner = ProactiveDeviceReasoner::new(store);
        let mut signal = base_signal();
        signal.time_of_day = NaiveTime::from_hms_opt(23, 30, 0).unwrap();
        signal.room_occupancy.insert("客厅".into(), false);
        signal.outdoor_temperature = Some(35.0);
        let suggestions = reasoner.evaluate(&signal);
        assert!(suggestions.is_empty());
    }
}
