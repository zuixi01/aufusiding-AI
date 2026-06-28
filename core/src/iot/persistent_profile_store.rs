//! 持久化语义档案存储
//!
//! 基于 SQLite 的设备语义档案存储，启动时加载，变更时写入。

use std::path::Path;

use chrono::Utc;

use crate::iot::semantic::profile::{DeviceSemanticProfile, SemanticProfileStore};
use crate::iot::types::Capability;
use crate::storage::db::Database;

/// 持久化语义档案存储
pub struct PersistentSemanticProfileStore {
    db: Database,
    inner: SemanticProfileStore,
}

impl PersistentSemanticProfileStore {
    /// 创建或打开持久化档案存储
    pub fn new(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db = Database::open(db_path.as_ref().to_path_buf())?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS semantic_profiles (
                device_id TEXT PRIMARY KEY,
                profile_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )?;
        let profiles = Self::load_all(&db)?;
        let mut inner = SemanticProfileStore::new();
        for p in profiles {
            inner.add(p);
        }
        Ok(Self { db, inner })
    }

    fn load_all(db: &Database) -> anyhow::Result<Vec<DeviceSemanticProfile>> {
        let rows: Vec<(String, String)> = db.query_map(
            "SELECT device_id, profile_json FROM semantic_profiles",
            &[],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut profiles = Vec::new();
        for (_id, json) in rows {
            if let Ok(p) = serde_json::from_str::<DeviceSemanticProfile>(&json) {
                profiles.push(p);
            }
        }
        Ok(profiles)
    }

    /// 添加档案（写入 SQLite + 内存）
    pub fn add(&mut self, profile: DeviceSemanticProfile) {
        let json = serde_json::to_string(&profile).unwrap_or_default();
        let now = Utc::now().timestamp();
        if let Err(e) = self.db.execute(
            "INSERT OR REPLACE INTO semantic_profiles (device_id, profile_json, updated_at) VALUES (?1, ?2, ?3)",
            &[&profile.device_id as &dyn rusqlite::types::ToSql, &json, &now],
        ) {
            tracing::warn!("persistent profile store write failed for {}: {}", profile.device_id, e);
        }
        self.inner.add(profile);
    }

    /// 获取档案
    pub fn get(&self, device_id: &str) -> Option<&DeviceSemanticProfile> {
        self.inner.get(device_id)
    }

    /// 删除档案
    pub fn remove(&mut self, device_id: &str) -> Option<DeviceSemanticProfile> {
        if let Err(e) = self.db.execute(
            "DELETE FROM semantic_profiles WHERE device_id = ?1",
            &[&device_id as &dyn rusqlite::types::ToSql],
        ) {
            tracing::warn!("persistent profile store delete failed for {}: {}", device_id, e);
        }
        self.inner.remove(device_id)
    }

    /// 列出所有档案
    pub fn list_all(&self) -> Vec<&DeviceSemanticProfile> {
        self.inner.list_all()
    }

    /// 按房间查询
    pub fn query_by_room(&self, room: &str) -> Vec<&DeviceSemanticProfile> {
        self.inner.query_by_room(room)
    }

    /// 按能力查询
    pub fn query_by_capability(&self, cap: &Capability) -> Vec<&DeviceSemanticProfile> {
        self.inner.query_by_capability(cap)
    }

    /// 按名称模糊搜索
    pub fn find_by_name(&self, name: &str) -> Vec<(&DeviceSemanticProfile, f64)> {
        self.inner.find_by_name(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::semantic::profile::build_default_profile;
    use crate::iot::types::{Capability, DeviceType};

    fn make_store() -> PersistentSemanticProfileStore {
        let path = crate::test_support::temp_db_path("persistent-profile");
        PersistentSemanticProfileStore::new(path).unwrap()
    }

    #[test]
    fn create_and_schema() {
        let _store = make_store();
    }

    #[test]
    fn add_and_get() {
        let mut store = make_store();
        let p = build_default_profile("light-001", "客厅灯", DeviceType::Light, Some("客厅"));
        store.add(p);
        let got = store.get("light-001").unwrap();
        assert_eq!(got.friendly_name, "客厅灯");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = make_store();
        assert!(store.get("no-such").is_none());
    }

    #[test]
    fn remove_deletes() {
        let mut store = make_store();
        store.add(build_default_profile("d1", "灯1", DeviceType::Light, None));
        assert!(store.get("d1").is_some());
        store.remove("d1");
        assert!(store.get("d1").is_none());
    }

    #[test]
    fn list_all_returns_all() {
        let mut store = make_store();
        store.add(build_default_profile("d1", "灯1", DeviceType::Light, None));
        store.add(build_default_profile("d2", "灯2", DeviceType::Light, None));
        assert_eq!(store.list_all().len(), 2);
    }

    #[test]
    fn query_by_room() {
        let mut store = make_store();
        store.add(build_default_profile("d1", "客厅灯", DeviceType::Light, Some("客厅")));
        store.add(build_default_profile("d2", "卧室灯", DeviceType::Light, Some("卧室")));
        let results = store.query_by_room("客厅");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].device_id, "d1");
    }

    #[test]
    fn query_by_capability() {
        let mut store = make_store();
        store.add(build_default_profile("d1", "灯", DeviceType::Light, None));
        let results = store.query_by_capability(&Capability::Switch);
        assert!(!results.is_empty());
    }

    #[test]
    fn find_by_name() {
        let mut store = make_store();
        store.add(build_default_profile("d1", "客厅灯", DeviceType::Light, Some("客厅")));
        store.add(build_default_profile("d2", "卧室灯", DeviceType::Light, Some("卧室")));
        let results = store.find_by_name("客厅灯");
        assert!(!results.is_empty());
        assert_eq!(results[0].0.device_id, "d1");
    }

    #[test]
    fn persistence_across_instances() {
        let path = crate::test_support::temp_db_path("profile-persist");
        {
            let mut store = PersistentSemanticProfileStore::new(&path).unwrap();
            store.add(build_default_profile("d1", "客厅灯", DeviceType::Light, Some("客厅")));
        }
        {
            let store = PersistentSemanticProfileStore::new(&path).unwrap();
            let got = store.get("d1").unwrap();
            assert_eq!(got.friendly_name, "客厅灯");
            assert_eq!(got.room.as_deref(), Some("客厅"));
        }
    }

    #[test]
    fn serde_roundtrip() {
        let p = build_default_profile("d1", "测试灯", DeviceType::Light, Some("测试房间"));
        let json = serde_json::to_string(&p).unwrap();
        let back: DeviceSemanticProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.device_id, "d1");
        assert_eq!(back.friendly_name, "测试灯");
        assert_eq!(back.room.as_deref(), Some("测试房间"));
    }
}
