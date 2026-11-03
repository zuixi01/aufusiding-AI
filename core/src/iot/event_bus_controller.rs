//! 事件总线控制器装饰器
//!
//! 包装任意 DeviceController，在命令执行成功后自动发布事件到 EventBus。

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::iot::control::DeviceController;
use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};
use crate::iot::types::DeviceType;

/// 装饰器：在设备命令执行后发布事件
pub struct EventBusController {
    inner: Box<dyn DeviceController>,
    event_bus: Arc<EventBus>,
}

impl EventBusController {
    pub fn new(inner: Box<dyn DeviceController>, event_bus: Arc<EventBus>) -> Self {
        Self { inner, event_bus }
    }

    fn device_type_enum(&self) -> DeviceType {
        match self.inner.supported_type() {
            "Light" => DeviceType::Light,
            "HomeAssistant" => DeviceType::Light,
            "SmartHome" => DeviceType::Light,
            "mqtt" => DeviceType::Sensor,
            "Thermostat" => DeviceType::Thermostat,
            "Lock" => DeviceType::Lock,
            "Speaker" => DeviceType::Speaker,
            _ => DeviceType::Sensor,
        }
    }

    fn publish(&self, device_id: &str, kind: EventKind, payload: Value) {
        let event = DeviceEvent::new(device_id, self.device_type_enum(), kind, payload);
        let receivers = self.event_bus.publish(event);
        if receivers == 0 {
            tracing::debug!("event bus: no active receivers for event");
        }
    }
}

#[async_trait]
impl DeviceController for EventBusController {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.inner.turn_on(device_id).await?;
        self.publish(
            device_id,
            EventKind::StateChanged,
            serde_json::json!({"action": "turn_on", "powered": true}),
        );
        Ok(())
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        self.inner.turn_off(device_id).await?;
        self.publish(
            device_id,
            EventKind::StateChanged,
            serde_json::json!({"action": "turn_off", "powered": false}),
        );
        Ok(())
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        self.inner
            .set_property(device_id, property, value.clone())
            .await?;
        self.publish(
            device_id,
            EventKind::PropertyChanged,
            serde_json::json!({"action": "set_property", "property": property, "value": value}),
        );
        Ok(())
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        self.inner.get_state(device_id).await
    }

    fn supported_type(&self) -> &str {
        self.inner.supported_type()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::control::MockDeviceController;

    fn setup() -> (EventBusController, Arc<EventBus>) {
        let bus = Arc::new(EventBus::new(16));
        let mock = MockDeviceController::new("Light");
        mock.register_device("light-001", "Light");
        let controller = EventBusController::new(Box::new(mock), bus.clone());
        (controller, bus)
    }

    #[tokio::test]
    async fn turn_on_publishes_state_changed() {
        let (ctrl, bus) = setup();
        let mut rx = bus.subscribe();
        ctrl.turn_on("light-001").await.unwrap();
        let event = rx.recv().await.unwrap();
        assert_eq!(event.device_id, "light-001");
        assert_eq!(event.event_kind, EventKind::StateChanged);
        assert_eq!(event.payload["action"], "turn_on");
        assert_eq!(event.payload["powered"], true);
    }

    #[tokio::test]
    async fn turn_off_publishes_state_changed() {
        let (ctrl, bus) = setup();
        let mut rx = bus.subscribe();
        ctrl.turn_off("light-001").await.unwrap();
        let event = rx.recv().await.unwrap();
        assert_eq!(event.device_id, "light-001");
        assert_eq!(event.event_kind, EventKind::StateChanged);
        assert_eq!(event.payload["action"], "turn_off");
        assert_eq!(event.payload["powered"], false);
    }

    #[tokio::test]
    async fn set_property_publishes_property_changed() {
        let (ctrl, bus) = setup();
        let mut rx = bus.subscribe();
        ctrl.set_property("light-001", "brightness", serde_json::json!(80))
            .await
            .unwrap();
        let event = rx.recv().await.unwrap();
        assert_eq!(event.device_id, "light-001");
        assert_eq!(event.event_kind, EventKind::PropertyChanged);
        assert_eq!(event.payload["property"], "brightness");
        assert_eq!(event.payload["value"], 80);
    }

    #[tokio::test]
    async fn get_state_does_not_publish() {
        let (ctrl, bus) = setup();
        let mut rx = bus.subscribe();
        let _state = ctrl.get_state("light-001").await.unwrap();
        // 尝试接收，应该超时（无事件）
        let result = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err()); // timeout = no event
    }

    #[tokio::test]
    async fn error_does_not_publish() {
        let bus = Arc::new(EventBus::new(16));
        let mock = MockDeviceController::new("Light");
        // 不注册设备，操作会失败
        let ctrl = EventBusController::new(Box::new(mock), bus.clone());
        let mut rx = bus.subscribe();
        let result = ctrl.turn_on("nonexistent").await;
        assert!(result.is_err());
        let recv = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(recv.is_err());
    }

    #[tokio::test]
    async fn event_contains_correct_device_id() {
        let (ctrl, bus) = setup();
        let mut rx = bus.subscribe();
        ctrl.turn_on("light-001").await.unwrap();
        let event = rx.recv().await.unwrap();
        assert_eq!(event.device_id, "light-001");
    }

    #[tokio::test]
    async fn supported_type_delegates() {
        let (ctrl, _bus) = setup();
        assert_eq!(ctrl.supported_type(), "Light");
    }

    #[tokio::test]
    async fn no_subscribers_does_not_panic() {
        let bus = Arc::new(EventBus::new(16));
        let mock = MockDeviceController::new("Light");
        mock.register_device("light-001", "Light");
        let ctrl = EventBusController::new(Box::new(mock), bus);
        // 无订阅者，不应 panic
        ctrl.turn_on("light-001").await.unwrap();
    }

    #[tokio::test]
    async fn multiple_events_sequential() {
        let (ctrl, bus) = setup();
        let mut rx = bus.subscribe();
        ctrl.turn_on("light-001").await.unwrap();
        ctrl.set_property("light-001", "brightness", serde_json::json!(50))
            .await
            .unwrap();
        ctrl.turn_off("light-001").await.unwrap();

        let e1 = rx.recv().await.unwrap();
        assert_eq!(e1.event_kind, EventKind::StateChanged);
        assert_eq!(e1.payload["action"], "turn_on");

        let e2 = rx.recv().await.unwrap();
        assert_eq!(e2.event_kind, EventKind::PropertyChanged);

        let e3 = rx.recv().await.unwrap();
        assert_eq!(e3.event_kind, EventKind::StateChanged);
        assert_eq!(e3.payload["action"], "turn_off");
    }
}
