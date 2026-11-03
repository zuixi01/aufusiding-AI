//! 设备状态缓存
//!
//! 监听 EventBus 的 PropertyChanged/StateChanged 事件，
//! 维护设备状态快照，供自动化条件评估使用。

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::event_bus::{EventBus, EventKind};

/// 设备状态缓存 — 合并事件 payload 到设备级快照
pub struct DeviceStateCache {
    states: Arc<RwLock<HashMap<String, Value>>>,
}

impl DeviceStateCache {
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取单设备状态
    pub async fn get(&self, device_id: &str) -> Option<Value> {
        self.states.read().await.get(device_id).cloned()
    }

    /// 手动设置设备状态（测试用）
    pub async fn set(&self, device_id: &str, state: Value) {
        self.states
            .write()
            .await
            .insert(device_id.to_string(), state);
    }

    /// 返回全量状态克隆
    pub async fn snapshot(&self) -> HashMap<String, Value> {
        self.states.read().await.clone()
    }

    /// 启动后台监听，合并 PropertyChanged/StateChanged 事件到缓存
    pub async fn start(&self, event_bus: &EventBus) {
        let states = Arc::clone(&self.states);
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                match event.event_kind {
                    EventKind::PropertyChanged | EventKind::StateChanged => {
                        let mut guard = states.write().await;
                        let entry = guard
                            .entry(event.device_id.clone())
                            .or_insert_with(|| Value::Object(serde_json::Map::new()));
                        // 合并 payload 字段到现有状态
                        if let (Some(existing), Some(incoming)) =
                            (entry.as_object_mut(), event.payload.as_object())
                        {
                            for (k, v) in incoming {
                                existing.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};
    use crate::iot::types::DeviceType;

    #[tokio::test]
    async fn set_and_get() {
        let cache = DeviceStateCache::new();
        cache
            .set("light-001", serde_json::json!({"brightness": 80}))
            .await;
        let state = cache.get("light-001").await.unwrap();
        assert_eq!(state["brightness"], 80);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let cache = DeviceStateCache::new();
        assert!(cache.get("no-such-device").await.is_none());
    }

    #[tokio::test]
    async fn snapshot_returns_all() {
        let cache = DeviceStateCache::new();
        cache.set("a", serde_json::json!({"on": true})).await;
        cache.set("b", serde_json::json!({"on": false})).await;
        let snap = cache.snapshot().await;
        assert_eq!(snap.len(), 2);
        assert_eq!(snap["a"]["on"], true);
        assert_eq!(snap["b"]["on"], false);
    }

    #[tokio::test]
    async fn merge_multiple_updates() {
        let cache = DeviceStateCache::new();
        cache
            .set("light-001", serde_json::json!({"brightness": 80}))
            .await;
        // Simulate merge by setting a combined object
        cache
            .set(
                "light-001",
                serde_json::json!({"brightness": 80, "color": "red"}),
            )
            .await;
        let state = cache.get("light-001").await.unwrap();
        assert_eq!(state["brightness"], 80);
        assert_eq!(state["color"], "red");
    }

    #[tokio::test]
    async fn start_merges_property_changed_events() {
        let bus = EventBus::new(64);
        let cache = DeviceStateCache::new();
        cache.start(&bus).await;

        // First event
        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 25}),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        // Second event — should merge
        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"humidity": 60}),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        let state = cache.get("sensor-001").await.unwrap();
        assert_eq!(state["temperature"], 25);
        assert_eq!(state["humidity"], 60);
    }

    #[tokio::test]
    async fn start_handles_state_changed() {
        let bus = EventBus::new(64);
        let cache = DeviceStateCache::new();
        cache.start(&bus).await;

        bus.publish(DeviceEvent::new(
            "light-001",
            DeviceType::Light,
            EventKind::StateChanged,
            serde_json::json!({"powered": true}),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        let state = cache.get("light-001").await.unwrap();
        assert_eq!(state["powered"], true);
    }

    #[tokio::test]
    async fn start_ignores_other_events() {
        let bus = EventBus::new(64);
        let cache = DeviceStateCache::new();
        cache.start(&bus).await;

        bus.publish(DeviceEvent::new(
            "light-001",
            DeviceType::Light,
            EventKind::DeviceDiscovered,
            serde_json::json!({"name": "Living Room Light"}),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        assert!(cache.get("light-001").await.is_none());
    }

    #[tokio::test]
    async fn concurrent_writes_safe() {
        let cache = Arc::new(DeviceStateCache::new());
        let mut handles = Vec::new();

        for i in 0..20 {
            let c = Arc::clone(&cache);
            handles.push(tokio::spawn(async move {
                c.set(&format!("dev-{}", i), serde_json::json!({"val": i}))
                    .await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let snap = cache.snapshot().await;
        assert_eq!(snap.len(), 20);
    }
}
