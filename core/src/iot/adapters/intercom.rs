//! 门禁对讲适配器
//!
//! 支持 SIP/RTSP/HTTP 协议的门禁对讲设备，提供门铃检测、双向音频、
//! 视频流、远程开锁等功能。实现 `DeviceController` trait。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::iot::control::DeviceController;
use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};
use crate::iot::types::DeviceType;

// ─── 类型定义 ────────────────────────────────────────────

/// 对讲协议
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntercomProtocol {
    Sip,
    Rtsp,
    Http,
}

/// 对讲事件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntercomEvent {
    Ring,
    Motion,
    Unlock,
    Lock,
}

/// 对讲能力声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntercomCapability {
    pub ring_detection: bool,
    pub two_way_audio: bool,
    pub video_feed: bool,
    pub remote_unlock: bool,
}

impl IntercomCapability {
    pub fn full() -> Self {
        Self { ring_detection: true, two_way_audio: true, video_feed: true, remote_unlock: true }
    }
}

/// 对讲设备配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntercomConfig {
    pub protocol: IntercomProtocol,
    pub host: String,
    pub capabilities: IntercomCapability,
}

/// 已注册设备内部状态
#[derive(Debug, Clone)]
struct IntercomDeviceState {
    config: IntercomConfig,
    locked: bool,
    last_event: Option<IntercomEvent>,
    last_event_time: Option<DateTime<Utc>>,
}

// ─── IntercomAdapter ─────────────────────────────────────

/// 门禁对讲适配器
pub struct IntercomAdapter {
    devices: Arc<Mutex<HashMap<String, IntercomDeviceState>>>,
    event_bus: Option<Arc<EventBus>>,
}

impl IntercomAdapter {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(Mutex::new(HashMap::new())),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn register_device(&self, device_id: &str, config: IntercomConfig) {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        devices.insert(device_id.to_string(), IntercomDeviceState {
            config,
            locked: true,
            last_event: None,
            last_event_time: None,
        });
    }

    fn publish_event(&self, device_id: &str, event: IntercomEvent) {
        if let Some(bus) = &self.event_bus {
            let kind = match event {
                IntercomEvent::Ring | IntercomEvent::Motion => EventKind::Alert,
                IntercomEvent::Unlock | IntercomEvent::Lock => EventKind::StateChanged,
            };
            bus.publish(DeviceEvent::new(
                device_id,
                DeviceType::Intercom,
                kind,
                serde_json::json!({ "intercom_event": event }),
            ));
        }
    }

    pub fn handle_ring(&self, device_id: &str) -> anyhow::Result<()> {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let state = devices.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown intercom: {}", device_id))?;
        state.last_event = Some(IntercomEvent::Ring);
        state.last_event_time = Some(Utc::now());
        drop(devices);
        self.publish_event(device_id, IntercomEvent::Ring);
        Ok(())
    }

    pub fn handle_motion(&self, device_id: &str) -> anyhow::Result<()> {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let state = devices.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown intercom: {}", device_id))?;
        state.last_event = Some(IntercomEvent::Motion);
        state.last_event_time = Some(Utc::now());
        drop(devices);
        self.publish_event(device_id, IntercomEvent::Motion);
        Ok(())
    }

    pub fn remote_unlock(&self, device_id: &str) -> anyhow::Result<()> {
        let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let state = devices.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown intercom: {}", device_id))?;
        if !state.config.capabilities.remote_unlock {
            anyhow::bail!("device {} does not support remote unlock", device_id);
        }
        state.locked = false;
        state.last_event = Some(IntercomEvent::Unlock);
        state.last_event_time = Some(Utc::now());
        drop(devices);
        self.publish_event(device_id, IntercomEvent::Unlock);
        Ok(())
    }

    pub fn get_video_feed_url(&self, device_id: &str) -> anyhow::Result<String> {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let state = devices.get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown intercom: {}", device_id))?;
        if !state.config.capabilities.video_feed {
            anyhow::bail!("device {} does not support video feed", device_id);
        }
        let url = match state.config.protocol {
            IntercomProtocol::Rtsp => format!("rtsp://{}/live", state.config.host),
            IntercomProtocol::Http => format!("http://{}/video/stream", state.config.host),
            IntercomProtocol::Sip => format!("sip://{}@{}", device_id, state.config.host),
        };
        Ok(url)
    }
}

#[async_trait]
impl DeviceController for IntercomAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        if !devices.contains_key(device_id) {
            anyhow::bail!("unknown intercom: {}", device_id);
        }
        Ok(())
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        if !devices.contains_key(device_id) {
            anyhow::bail!("unknown intercom: {}", device_id);
        }
        Ok(())
    }

    async fn set_property(&self, device_id: &str, property: &str, value: Value) -> anyhow::Result<()> {
        match property {
            "locked" => {
                let lock = value.as_bool().unwrap_or(true);
                let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
                let state = devices.get_mut(device_id)
                    .ok_or_else(|| anyhow::anyhow!("unknown intercom: {}", device_id))?;
                state.locked = lock;
                Ok(())
            }
            _ => anyhow::bail!("unsupported property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let state = devices.get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown intercom: {}", device_id))?;
        Ok(serde_json::json!({
            "device_id": device_id,
            "device_type": "intercom",
            "locked": state.locked,
            "last_event": state.last_event,
            "protocol": state.config.protocol,
        }))
    }

    fn supported_type(&self) -> &str {
        "intercom"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> IntercomConfig {
        IntercomConfig {
            protocol: IntercomProtocol::Rtsp,
            host: "192.168.1.100".into(),
            capabilities: IntercomCapability::full(),
        }
    }

    fn setup() -> IntercomAdapter {
        let adapter = IntercomAdapter::new();
        adapter.register_device("intercom-001", test_config());
        adapter
    }

    #[test]
    fn register_and_supported_type() {
        let adapter = setup();
        assert_eq!(adapter.supported_type(), "intercom");
    }

    #[tokio::test]
    async fn get_state_returns_info() {
        let adapter = setup();
        let state = adapter.get_state("intercom-001").await.unwrap();
        assert_eq!(state["device_type"], "intercom");
        assert_eq!(state["locked"], true);
    }

    #[tokio::test]
    async fn unknown_device_errors() {
        let adapter = IntercomAdapter::new();
        assert!(adapter.get_state("nope").await.is_err());
    }

    #[test]
    fn handle_ring_updates_state() {
        let adapter = setup();
        adapter.handle_ring("intercom-001").unwrap();
        let devices = adapter.devices.lock().unwrap();
        let s = devices.get("intercom-001").unwrap();
        assert_eq!(s.last_event, Some(IntercomEvent::Ring));
    }

    #[test]
    fn handle_motion_updates_state() {
        let adapter = setup();
        adapter.handle_motion("intercom-001").unwrap();
        let devices = adapter.devices.lock().unwrap();
        let s = devices.get("intercom-001").unwrap();
        assert_eq!(s.last_event, Some(IntercomEvent::Motion));
    }

    #[test]
    fn remote_unlock_works() {
        let adapter = setup();
        adapter.remote_unlock("intercom-001").unwrap();
        let devices = adapter.devices.lock().unwrap();
        let s = devices.get("intercom-001").unwrap();
        assert!(!s.locked);
        assert_eq!(s.last_event, Some(IntercomEvent::Unlock));
    }

    #[test]
    fn remote_unlock_no_capability() {
        let adapter = IntercomAdapter::new();
        adapter.register_device("intercom-002", IntercomConfig {
            protocol: IntercomProtocol::Http,
            host: "192.168.1.101".into(),
            capabilities: IntercomCapability {
                ring_detection: true,
                two_way_audio: false,
                video_feed: false,
                remote_unlock: false,
            },
        });
        assert!(adapter.remote_unlock("intercom-002").is_err());
    }

    #[test]
    fn video_feed_url_rtsp() {
        let adapter = setup();
        let url = adapter.get_video_feed_url("intercom-001").unwrap();
        assert!(url.starts_with("rtsp://"));
        assert!(url.contains("192.168.1.100"));
    }

    #[test]
    fn video_feed_url_http() {
        let adapter = IntercomAdapter::new();
        adapter.register_device("ic-http", IntercomConfig {
            protocol: IntercomProtocol::Http,
            host: "10.0.0.1".into(),
            capabilities: IntercomCapability::full(),
        });
        let url = adapter.get_video_feed_url("ic-http").unwrap();
        assert!(url.starts_with("http://"));
    }

    #[test]
    fn video_feed_no_capability() {
        let adapter = IntercomAdapter::new();
        adapter.register_device("ic-novid", IntercomConfig {
            protocol: IntercomProtocol::Sip,
            host: "10.0.0.2".into(),
            capabilities: IntercomCapability {
                ring_detection: true, two_way_audio: true,
                video_feed: false, remote_unlock: true,
            },
        });
        assert!(adapter.get_video_feed_url("ic-novid").is_err());
    }

    #[test]
    fn event_bus_receives_ring() {
        let bus = Arc::new(EventBus::new(16));
        let mut rx = bus.subscribe();
        let adapter = IntercomAdapter::new().with_event_bus(Arc::clone(&bus));
        adapter.register_device("ic-bus", test_config());
        adapter.handle_ring("ic-bus").unwrap();
        let event = rx.try_recv().unwrap();
        assert_eq!(event.device_id, "ic-bus");
        assert_eq!(event.event_kind, EventKind::Alert);
    }

    #[tokio::test]
    async fn set_property_locked() {
        let adapter = setup();
        adapter.set_property("intercom-001", "locked", serde_json::json!(false)).await.unwrap();
        let state = adapter.get_state("intercom-001").await.unwrap();
        assert_eq!(state["locked"], false);
    }

    #[tokio::test]
    async fn set_property_unsupported() {
        let adapter = setup();
        assert!(adapter.set_property("intercom-001", "color", serde_json::json!("red")).await.is_err());
    }

    #[test]
    fn intercom_protocol_serde_roundtrip() {
        let p = IntercomProtocol::Sip;
        let json = serde_json::to_string(&p).unwrap();
        let back: IntercomProtocol = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}
