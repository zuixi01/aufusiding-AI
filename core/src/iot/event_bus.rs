//! 设备事件总线
//!
//! 基于 `tokio::sync::broadcast` 的异步事件总线，
//! 用于设备状态变更、发现、上下线等事件的发布/订阅。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

use super::types::DeviceType;

/// 事件类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// 设备状态变更
    StateChanged,
    /// 发现新设备
    DeviceDiscovered,
    /// 设备上线
    DeviceOnline,
    /// 设备离线
    DeviceOffline,
    /// 设备已配对
    DevicePaired,
    /// 设备已解除配对
    DeviceUnpaired,
    /// 属性变更
    PropertyChanged,
    /// 告警
    Alert,
    /// 错误
    Error,
}

/// 设备事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEvent {
    pub device_id: String,
    pub device_type: DeviceType,
    pub event_kind: EventKind,
    pub payload: Value,
    pub timestamp: DateTime<Utc>,
}

impl DeviceEvent {
    /// 创建新事件
    pub fn new(
        device_id: impl Into<String>,
        device_type: DeviceType,
        event_kind: EventKind,
        payload: Value,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            device_type,
            event_kind,
            payload,
            timestamp: Utc::now(),
        }
    }
}

/// 事件总线 — broadcast channel
pub struct EventBus {
    sender: broadcast::Sender<DeviceEvent>,
}

impl EventBus {
    /// 创建事件总线，capacity 为 channel 缓冲区大小
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// 发布事件，返回接收到的订阅者数量
    pub fn publish(&self, event: DeviceEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// 订阅事件流
    pub fn subscribe(&self) -> broadcast::Receiver<DeviceEvent> {
        self.sender.subscribe()
    }

    /// 当前订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> DeviceEvent {
        DeviceEvent::new(
            "light-001",
            DeviceType::Light,
            EventKind::StateChanged,
            serde_json::json!({"powered": true}),
        )
    }

    #[test]
    fn event_bus_no_subscribers() {
        let bus = EventBus::new(16);
        let count = bus.publish(sample_event());
        assert_eq!(count, 0);
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn event_bus_single_subscriber() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        bus.publish(sample_event());
        let event = rx.recv().await.unwrap();
        assert_eq!(event.device_id, "light-001");
        assert_eq!(event.event_kind, EventKind::StateChanged);
    }

    #[tokio::test]
    async fn event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        let count = bus.publish(sample_event());
        assert_eq!(count, 2);

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.device_id, e2.device_id);
    }

    #[test]
    fn device_event_new_sets_timestamp() {
        let event = sample_event();
        assert!(!event.device_id.is_empty());
        assert!(event.timestamp <= Utc::now());
    }

    #[test]
    fn event_kind_serde_roundtrip() {
        let kind = EventKind::DeviceDiscovered;
        let json = serde_json::to_string(&kind).unwrap();
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, back);
    }
}
