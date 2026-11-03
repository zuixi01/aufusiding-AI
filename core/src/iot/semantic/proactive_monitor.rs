//! 主动推理监控器
//!
//! 订阅 EventBus，维护设备状态快照，定期评估规则生成建议。

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Local};
use serde_json::Value;
use tokio::sync::broadcast;

use super::proactive::{DeviceContextSignal, ProactiveDeviceReasoner, ProactiveSuggestion};
use super::profile::SemanticProfileStore;
use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};

/// 主动推理监控器
pub struct ProactiveMonitor {
    receiver: broadcast::Receiver<DeviceEvent>,
    reasoner: ProactiveDeviceReasoner,
    device_states: HashMap<String, Value>,
    room_occupancy: HashMap<String, bool>,
    outdoor_temperature: Option<f64>,
}

impl ProactiveMonitor {
    pub fn new(bus: &EventBus, store: Arc<SemanticProfileStore>) -> Self {
        Self {
            receiver: bus.subscribe(),
            reasoner: ProactiveDeviceReasoner::new(store),
            device_states: HashMap::new(),
            room_occupancy: HashMap::new(),
            outdoor_temperature: None,
        }
    }

    /// 消费所有待处理事件，更新内部状态快照
    pub fn process_pending(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            self.apply_event(&event);
        }
    }

    /// 获取设备状态快照
    pub fn device_states(&self) -> &HashMap<String, Value> {
        &self.device_states
    }
    /// 获取房间占用状态
    pub fn room_occupancy(&self) -> &HashMap<String, bool> {
        &self.room_occupancy
    }

    /// 设置室外温度
    pub fn set_outdoor_temperature(&mut self, temp: f64) {
        self.outdoor_temperature = Some(temp);
    }

    /// 设置房间占用状态
    pub fn set_room_occupancy(&mut self, room: &str, occupied: bool) {
        self.room_occupancy.insert(room.to_string(), occupied);
    }

    /// 评估当前状态，生成主动建议
    pub fn evaluate(&self) -> Vec<ProactiveSuggestion> {
        self.evaluate_at(Local::now())
    }

    fn evaluate_at(&self, now: DateTime<Local>) -> Vec<ProactiveSuggestion> {
        let signal = DeviceContextSignal {
            time_of_day: now.time(),
            day_of_week: now.weekday(),
            room_occupancy: self.room_occupancy.clone(),
            device_states: self.device_states.clone(),
            outdoor_temperature: self.outdoor_temperature,
        };
        self.reasoner.evaluate(&signal)
    }

    fn apply_event(&mut self, event: &DeviceEvent) {
        match &event.event_kind {
            EventKind::StateChanged => {
                let powered = event
                    .payload
                    .get("powered")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let state = self
                    .device_states
                    .entry(event.device_id.clone())
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(obj) = state.as_object_mut() {
                    obj.insert("powered".to_string(), Value::Bool(powered));
                }
            }
            EventKind::PropertyChanged => {
                if let (Some(prop), Some(val)) = (
                    event.payload.get("property").and_then(|v| v.as_str()),
                    event.payload.get("value"),
                ) {
                    // 特殊处理：温度传感器更新室外温度
                    if prop == "outdoor_temperature" {
                        if let Some(t) = val.as_f64() {
                            self.outdoor_temperature = Some(t);
                        }
                        return;
                    }
                    // 特殊处理：占用传感器
                    if prop == "occupancy" {
                        if let Some(room) = event.payload.get("room").and_then(|v| v.as_str()) {
                            let occupied = val.as_bool().unwrap_or(false);
                            self.room_occupancy.insert(room.to_string(), occupied);
                        }
                        return;
                    }
                    let state = self
                        .device_states
                        .entry(event.device_id.clone())
                        .or_insert_with(|| serde_json::json!({"properties": {}}));
                    if let Some(obj) = state.as_object_mut() {
                        let props = obj
                            .entry("properties")
                            .or_insert_with(|| serde_json::json!({}));
                        if let Some(props_obj) = props.as_object_mut() {
                            props_obj.insert(prop.to_string(), val.clone());
                        }
                    }
                }
            }
            EventKind::DeviceOffline => {
                self.device_states.remove(&event.device_id);
            }
            _ => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::profile::{build_default_profile, SemanticProfileStore};
    use super::*;
    use crate::iot::event_bus::EventBus;
    use crate::iot::types::DeviceType;
    use chrono::TimeZone;

    fn fixed_local_time(hour: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, 4, hour, 0, 0)
            .single()
            .expect("fixed local test time should be unambiguous")
    }

    fn make_store() -> Arc<SemanticProfileStore> {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile(
            "light-001",
            "客厅灯",
            DeviceType::Light,
            Some("客厅"),
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

    fn state_event(device_id: &str, powered: bool) -> DeviceEvent {
        DeviceEvent::new(
            device_id,
            DeviceType::Light,
            EventKind::StateChanged,
            serde_json::json!({"action": if powered { "turn_on" } else { "turn_off" }, "powered": powered}),
        )
    }

    fn property_event(device_id: &str, prop: &str, val: Value) -> DeviceEvent {
        DeviceEvent::new(
            device_id,
            DeviceType::Light,
            EventKind::PropertyChanged,
            serde_json::json!({"property": prop, "value": val}),
        )
    }

    #[test]
    fn state_event_updates_device_states() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        bus.publish(state_event("light-001", true));
        monitor.process_pending();
        let state = monitor.device_states().get("light-001").unwrap();
        assert_eq!(state.get("powered").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn property_event_updates_properties() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        bus.publish(property_event(
            "light-001",
            "brightness",
            serde_json::json!(80),
        ));
        monitor.process_pending();
        let state = monitor.device_states().get("light-001").unwrap();
        assert_eq!(
            state
                .get("properties")
                .unwrap()
                .get("brightness")
                .unwrap()
                .as_i64(),
            Some(80)
        );
    }

    #[test]
    fn occupancy_event_updates_room() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        let event = DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"property": "occupancy", "value": true, "room": "客厅"}),
        );
        bus.publish(event);
        monitor.process_pending();
        assert_eq!(monitor.room_occupancy().get("客厅"), Some(&true));
    }
    #[test]
    fn outdoor_temperature_event_updates() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        let event = DeviceEvent::new(
            "weather-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"property": "outdoor_temperature", "value": 33.5}),
        );
        bus.publish(event);
        monitor.process_pending();
        assert!((monitor.outdoor_temperature.unwrap() - 33.5).abs() < 0.01);
    }

    #[test]
    fn device_offline_clears_state() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        bus.publish(state_event("light-001", true));
        monitor.process_pending();
        assert!(monitor.device_states().contains_key("light-001"));
        let offline = DeviceEvent::new(
            "light-001",
            DeviceType::Light,
            EventKind::DeviceOffline,
            serde_json::json!({}),
        );
        bus.publish(offline);
        monitor.process_pending();
        assert!(!monitor.device_states().contains_key("light-001"));
    }

    #[test]
    fn multiple_events_accumulate() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        bus.publish(state_event("light-001", true));
        bus.publish(state_event("thermo-001", false));
        bus.publish(property_event(
            "light-001",
            "brightness",
            serde_json::json!(60),
        ));
        monitor.process_pending();
        assert_eq!(monitor.device_states().len(), 2);
        let light = monitor.device_states().get("light-001").unwrap();
        assert_eq!(light.get("powered").unwrap().as_bool(), Some(true));
        assert_eq!(
            light
                .get("properties")
                .unwrap()
                .get("brightness")
                .unwrap()
                .as_i64(),
            Some(60)
        );
    }

    #[test]
    fn evaluate_returns_empty_when_no_issues() {
        let bus = EventBus::new(16);
        let store = make_store();
        let monitor = ProactiveMonitor::new(&bus, store);
        let suggestions = monitor.evaluate_at(fixed_local_time(14));
        // 日间无设备状态、占用或温度信号时，不产生建议。
        assert!(suggestions.is_empty());
    }

    #[test]
    fn weekday_morning_without_device_states_suggests_lights() {
        let bus = EventBus::new(16);
        let store = make_store();
        let monitor = ProactiveMonitor::new(&bus, store);
        let suggestions = monitor.evaluate_at(fixed_local_time(7));

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].actions.len(), 1);
        assert_eq!(suggestions[0].actions[0].device_id, "light-001");
        assert_eq!(suggestions[0].actions[0].command, "turn_on");
    }

    #[test]
    fn set_room_occupancy_works() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        monitor.set_room_occupancy("客厅", false);
        assert_eq!(monitor.room_occupancy().get("客厅"), Some(&false));
        monitor.set_room_occupancy("客厅", true);
        assert_eq!(monitor.room_occupancy().get("客厅"), Some(&true));
    }

    #[test]
    fn set_outdoor_temperature_works() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        assert!(monitor.outdoor_temperature.is_none());
        monitor.set_outdoor_temperature(28.5);
        assert!((monitor.outdoor_temperature.unwrap() - 28.5).abs() < 0.01);
    }

    #[test]
    fn empty_room_with_light_on_generates_suggestion() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        bus.publish(state_event("light-001", true));
        monitor.process_pending();
        monitor.set_room_occupancy("客厅", false);
        let suggestions = monitor.evaluate_at(fixed_local_time(14));
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.description.contains("客厅")));
    }

    #[test]
    fn no_pending_events_process_is_noop() {
        let bus = EventBus::new(16);
        let store = make_store();
        let mut monitor = ProactiveMonitor::new(&bus, store);
        monitor.process_pending();
        assert!(monitor.device_states().is_empty());
    }
}
