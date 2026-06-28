//! 持久化设备状态缓存
//!
//! 基于 SQLite 的设备状态缓存，启动时加载，变更时写入。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::storage::db::Database;
use super::event_bus::{EventBus, EventKind};

/// 持久化设备状态缓存
pub struct PersistentDeviceStateCache {
    db: Database,
    states: Arc<RwLock<HashMap<String, Value>>>,
}

impl PersistentDeviceStateCache {
    /// 创建或打开持久化缓存
    pub fn new(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db = Database::open(db_path.as_ref().to_path_buf())?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS device_states (
                device_id TEXT PRIMARY KEY,
                state_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )?;
        let states = Self::load_all(&db)?;
        Ok(Self {
            db,
            states: Arc::new(RwLock::new(states)),
        })
    }

    /// 从数据库加载所有状态
    fn load_all(db: &Database) -> anyhow::Result<HashMap<String, Value>> {
        let rows: Vec<(String, String)> = db.query_map(
            "SELECT device_id, state_json FROM device_states",
            &[],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut map = HashMap::new();
        for (id, json) in rows {
            if let Ok(val) = serde_json::from_str(&json) {
                map.insert(id, val);
            }
        }
        Ok(map)
    }

    /// 获取单设备状态
    pub async fn get(&self, device_id: &str) -> Option<Value> {
        self.states.read().await.get(device_id).cloned()
    }

    /// 设置设备状态（写入 SQLite + 内存）
    pub async fn set(&self, device_id: &str, state: Value) {
        let json = serde_json::to_string(&state).unwrap_or_default();
        let now = Utc::now().timestamp();
        if let Err(e) = self.db.execute(
            "INSERT OR REPLACE INTO device_states (device_id, state_json, updated_at) VALUES (?1, ?2, ?3)",
            &[&device_id as &dyn rusqlite::types::ToSql, &json, &now],
        ) {
            tracing::warn!("persistent state cache write failed for {}: {}", device_id, e);
        }
        self.states.write().await.insert(device_id.to_string(), state);
    }

    /// 返回全量状态克隆
    pub async fn snapshot(&self) -> HashMap<String, Value> {
        self.states.read().await.clone()
    }

    /// 启动后台监听，合并事件并持久化
    pub async fn start(&self, event_bus: &EventBus) {
        let states = Arc::clone(&self.states);
        let db_path = self.db.path().to_path_buf();
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            let db = match Database::open(db_path) {
                Ok(d) => d,
                Err(_) => return,
            };
            while let Ok(event) = rx.recv().await {
                match event.event_kind {
                    EventKind::PropertyChanged | EventKind::StateChanged => {
                        let mut guard = states.write().await;
                        let entry = guard
                            .entry(event.device_id.clone())
                            .or_insert_with(|| Value::Object(serde_json::Map::new()));
                        if let (Some(existing), Some(incoming)) =
                            (entry.as_object_mut(), event.payload.as_object())
                        {
                            for (k, v) in incoming {
                                existing.insert(k.clone(), v.clone());
                            }
                        }
                        // 持久化
                        let json = serde_json::to_string(&entry).unwrap_or_default();
                        let now = Utc::now().timestamp();
                        if let Err(e) = db.execute(
                            "INSERT OR REPLACE INTO device_states (device_id, state_json, updated_at) VALUES (?1, ?2, ?3)",
                            &[&event.device_id as &dyn rusqlite::types::ToSql, &json, &now],
                        ) {
                            tracing::warn!("persistent state cache event write failed: {}", e);
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

    fn make_cache() -> PersistentDeviceStateCache {
        let path = crate::test_support::temp_db_path("persistent-state");
        PersistentDeviceStateCache::new(path).unwrap()
    }

    #[test]
    fn create_and_schema() {
        let _cache = make_cache();
    }

    #[tokio::test]
    async fn set_and_get() {
        let cache = make_cache();
        cache.set("light-001", serde_json::json!({"brightness": 80})).await;
        let state = cache.get("light-001").await.unwrap();
        assert_eq!(state["brightness"], 80);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let cache = make_cache();
        assert!(cache.get("no-such").await.is_none());
    }

    #[tokio::test]
    async fn snapshot_returns_all() {
        let cache = make_cache();
        cache.set("a", serde_json::json!({"on": true})).await;
        cache.set("b", serde_json::json!({"on": false})).await;
        let snap = cache.snapshot().await;
        assert_eq!(snap.len(), 2);
    }

    #[tokio::test]
    async fn overwrite_updates() {
        let cache = make_cache();
        cache.set("d1", serde_json::json!({"v": 1})).await;
        cache.set("d1", serde_json::json!({"v": 2})).await;
        let state = cache.get("d1").await.unwrap();
        assert_eq!(state["v"], 2);
    }

    #[tokio::test]
    async fn persistence_across_instances() {
        let path = crate::test_support::temp_db_path("persist-cross");
        {
            let cache = PersistentDeviceStateCache::new(&path).unwrap();
            cache.set("dev-1", serde_json::json!({"temp": 25})).await;
        }
        {
            let cache = PersistentDeviceStateCache::new(&path).unwrap();
            let state = cache.get("dev-1").await.unwrap();
            assert_eq!(state["temp"], 25);
        }
    }

    #[tokio::test]
    async fn start_merges_and_persists() {
        let path = crate::test_support::temp_db_path("persist-events");
        let bus = EventBus::new(64);
        {
            let cache = PersistentDeviceStateCache::new(&path).unwrap();
            cache.start(&bus).await;
            bus.publish(DeviceEvent::new(
                "s1", DeviceType::Sensor,
                EventKind::PropertyChanged,
                serde_json::json!({"temperature": 22}),
            ));
            tokio::task::yield_now().await;
            tokio::task::yield_now().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        // 重新打开验证持久化
        let cache2 = PersistentDeviceStateCache::new(&path).unwrap();
        let state = cache2.get("s1").await;
        assert!(state.is_some());
        assert_eq!(state.unwrap()["temperature"], 22);
    }

    #[tokio::test]
    async fn load_on_startup() {
        let path = crate::test_support::temp_db_path("persist-load");
        let cache = PersistentDeviceStateCache::new(&path).unwrap();
        cache.set("x", serde_json::json!({"a": 1})).await;
        drop(cache);
        let cache2 = PersistentDeviceStateCache::new(&path).unwrap();
        assert!(cache2.get("x").await.is_some());
    }

    #[tokio::test]
    async fn concurrent_writes_safe() {
        let cache = Arc::new(make_cache());
        let mut handles = Vec::new();
        for i in 0..10 {
            let c = Arc::clone(&cache);
            handles.push(tokio::spawn(async move {
                c.set(&format!("dev-{}", i), serde_json::json!({"v": i})).await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let snap = cache.snapshot().await;
        assert_eq!(snap.len(), 10);
    }

    #[tokio::test]
    async fn merge_preserves_fields() {
        let path = crate::test_support::temp_db_path("persist-merge");
        let bus = EventBus::new(64);
        let cache = PersistentDeviceStateCache::new(&path).unwrap();
        cache.start(&bus).await;

        bus.publish(DeviceEvent::new(
            "s1", DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 22}),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;

        bus.publish(DeviceEvent::new(
            "s1", DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"humidity": 60}),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;

        let state = cache.get("s1").await.unwrap();
        assert_eq!(state["temperature"], 22);
        assert_eq!(state["humidity"], 60);
    }
}
