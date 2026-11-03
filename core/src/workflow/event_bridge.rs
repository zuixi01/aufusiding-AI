//! 工作流事件桥接
//!
//! 订阅 EventBus，将设备事件转换为 TriggerContext，
//! 检查并触发匹配的工作流。

use serde_json::Value;

use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};
use crate::workflow::triggers::{DeviceStateChange, TriggerContext};

/// 从 DeviceEvent 构建 TriggerContext
pub fn build_trigger_context(event: &DeviceEvent) -> TriggerContext {
    let state_str = extract_state_string(&event.payload, &event.event_kind);
    TriggerContext {
        current_time: event.timestamp,
        user_input: None,
        event_type: Some(event_kind_to_string(&event.event_kind)),
        event_data: Some(event.payload.to_string()),
        webhook_path: None,
        device_state: Some(DeviceStateChange {
            device_id: event.device_id.clone(),
            state: state_str,
        }),
        geofence_transition: None,
    }
}

/// 从事件 payload 提取状态字符串
fn extract_state_string(payload: &Value, kind: &EventKind) -> String {
    match kind {
        EventKind::StateChanged => {
            if let Some(action) = payload.get("action").and_then(|v| v.as_str()) {
                match action {
                    "turn_on" => "on".to_string(),
                    "turn_off" => "off".to_string(),
                    _ => action.to_string(),
                }
            } else if let Some(powered) = payload.get("powered").and_then(|v| v.as_bool()) {
                if powered {
                    "on".to_string()
                } else {
                    "off".to_string()
                }
            } else {
                "unknown".to_string()
            }
        }
        EventKind::PropertyChanged => {
            if let Some(prop) = payload.get("property").and_then(|v| v.as_str()) {
                let val = payload
                    .get("value")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                format!("{}={}", prop, val)
            } else {
                "changed".to_string()
            }
        }
        _ => format!("{:?}", kind).to_lowercase(),
    }
}

/// EventKind → 字符串
fn event_kind_to_string(kind: &EventKind) -> String {
    match kind {
        EventKind::StateChanged => "state_changed".to_string(),
        EventKind::PropertyChanged => "property_changed".to_string(),
        EventKind::DeviceDiscovered => "device_discovered".to_string(),
        EventKind::DeviceOnline => "device_online".to_string(),
        EventKind::DeviceOffline => "device_offline".to_string(),
        EventKind::DevicePaired => "device_paired".to_string(),
        EventKind::DeviceUnpaired => "device_unpaired".to_string(),
        EventKind::Alert => "alert".to_string(),
        EventKind::Error => "error".to_string(),
    }
}

/// 工作流事件桥接器
///
/// 订阅 EventBus，将设备事件转换为 TriggerContext，
/// 供 WorkflowEngine 检查触发条件。
pub struct WorkflowEventBridge {
    receiver: tokio::sync::broadcast::Receiver<DeviceEvent>,
}

impl WorkflowEventBridge {
    pub fn new(bus: &EventBus) -> Self {
        Self {
            receiver: bus.subscribe(),
        }
    }

    /// 消费所有待处理事件，返回对应的 TriggerContext 列表
    pub fn drain_contexts(&mut self) -> Vec<TriggerContext> {
        let mut contexts = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            contexts.push(build_trigger_context(&event));
        }
        contexts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};
    use crate::iot::types::DeviceType;
    use crate::workflow::triggers::Trigger;
    use std::sync::Arc;

    fn make_state_event(device_id: &str, action: &str) -> DeviceEvent {
        let powered = action == "turn_on";
        DeviceEvent::new(
            device_id,
            DeviceType::Light,
            EventKind::StateChanged,
            serde_json::json!({"action": action, "powered": powered}),
        )
    }

    fn make_property_event(device_id: &str, prop: &str, val: i64) -> DeviceEvent {
        DeviceEvent::new(
            device_id,
            DeviceType::Light,
            EventKind::PropertyChanged,
            serde_json::json!({"action": "set_property", "property": prop, "value": val}),
        )
    }

    #[test]
    fn build_context_from_turn_on_event() {
        let event = make_state_event("light-001", "turn_on");
        let ctx = build_trigger_context(&event);
        assert_eq!(ctx.event_type.as_deref(), Some("state_changed"));
        let ds = ctx.device_state.unwrap();
        assert_eq!(ds.device_id, "light-001");
        assert_eq!(ds.state, "on");
    }

    #[test]
    fn build_context_from_turn_off_event() {
        let event = make_state_event("light-001", "turn_off");
        let ctx = build_trigger_context(&event);
        let ds = ctx.device_state.unwrap();
        assert_eq!(ds.state, "off");
    }

    #[test]
    fn build_context_from_property_event() {
        let event = make_property_event("light-001", "brightness", 80);
        let ctx = build_trigger_context(&event);
        assert_eq!(ctx.event_type.as_deref(), Some("property_changed"));
        let ds = ctx.device_state.unwrap();
        assert_eq!(ds.device_id, "light-001");
        assert!(ds.state.contains("brightness"));
        assert!(ds.state.contains("80"));
    }

    #[test]
    fn device_state_trigger_matches_on_event() {
        let trigger = Trigger::DeviceState {
            device_id: "light-001".into(),
            state: "on".into(),
        };
        let event = make_state_event("light-001", "turn_on");
        let ctx = build_trigger_context(&event);
        assert!(trigger.check(&ctx));
    }

    #[test]
    fn device_state_trigger_no_match_wrong_state() {
        let trigger = Trigger::DeviceState {
            device_id: "light-001".into(),
            state: "off".into(),
        };
        let event = make_state_event("light-001", "turn_on");
        let ctx = build_trigger_context(&event);
        assert!(!trigger.check(&ctx));
    }

    #[test]
    fn device_state_trigger_no_match_wrong_device() {
        let trigger = Trigger::DeviceState {
            device_id: "light-002".into(),
            state: "on".into(),
        };
        let event = make_state_event("light-001", "turn_on");
        let ctx = build_trigger_context(&event);
        assert!(!trigger.check(&ctx));
    }

    #[test]
    fn event_trigger_matches_state_changed() {
        let trigger = Trigger::Event {
            event_type: "state_changed".into(),
        };
        let event = make_state_event("light-001", "turn_on");
        let ctx = build_trigger_context(&event);
        assert!(trigger.check(&ctx));
    }

    #[test]
    fn event_kind_to_string_all_variants() {
        assert_eq!(
            event_kind_to_string(&EventKind::StateChanged),
            "state_changed"
        );
        assert_eq!(
            event_kind_to_string(&EventKind::PropertyChanged),
            "property_changed"
        );
        assert_eq!(
            event_kind_to_string(&EventKind::DeviceOnline),
            "device_online"
        );
        assert_eq!(
            event_kind_to_string(&EventKind::DeviceOffline),
            "device_offline"
        );
        assert_eq!(event_kind_to_string(&EventKind::Alert), "alert");
    }

    #[tokio::test]
    async fn bridge_drains_multiple_events() {
        let bus = Arc::new(EventBus::new(16));
        let mut bridge = WorkflowEventBridge::new(&bus);

        bus.publish(make_state_event("light-001", "turn_on"));
        bus.publish(make_property_event("light-001", "brightness", 50));
        bus.publish(make_state_event("light-001", "turn_off"));

        let contexts = bridge.drain_contexts();
        assert_eq!(contexts.len(), 3);
        assert_eq!(contexts[0].device_state.as_ref().unwrap().state, "on");
        assert!(contexts[1]
            .device_state
            .as_ref()
            .unwrap()
            .state
            .contains("brightness"));
        assert_eq!(contexts[2].device_state.as_ref().unwrap().state, "off");
    }

    #[tokio::test]
    async fn bridge_empty_when_no_events() {
        let bus = Arc::new(EventBus::new(16));
        let mut bridge = WorkflowEventBridge::new(&bus);
        let contexts = bridge.drain_contexts();
        assert!(contexts.is_empty());
    }

    #[test]
    fn property_changed_event_contains_property_name() {
        let event = make_property_event("thermo-001", "temperature", 25);
        let ctx = build_trigger_context(&event);
        assert!(ctx.event_data.as_ref().unwrap().contains("temperature"));
    }
}
