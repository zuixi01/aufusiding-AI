//! 设备注册表
//!
//! SQLite 持久化已发现/已配对设备。

use crate::storage::db::Database;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 注册设备信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisteredDevice {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub ip: String,
    pub port: u16,
    pub protocol: String,
    pub paired: bool,
    pub paired_at: Option<i64>,
}

/// 设备注册表（SQLite 持久化）
pub struct DeviceRegistry {
    db: Database,
}

impl DeviceRegistry {
    /// 创建设备注册表
    pub fn new(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db = Database::open(db_path.as_ref().to_path_buf())?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                device_type TEXT NOT NULL,
                ip TEXT NOT NULL,
                port INTEGER NOT NULL,
                protocol TEXT NOT NULL,
                paired INTEGER NOT NULL DEFAULT 0,
                paired_at INTEGER
            )",
        )?;
        Ok(Self { db })
    }

    /// 注册设备
    pub fn register(&self, device: &RegisteredDevice) -> anyhow::Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO devices (id, name, device_type, ip, port, protocol, paired, paired_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[
                &device.id,
                &device.name,
                &device.device_type,
                &device.ip,
                &(device.port as i32),
                &device.protocol,
                &device.paired,
                &device.paired_at,
            ],
        )?;
        Ok(())
    }

    /// 注销设备
    pub fn unregister(&self, device_id: &str) -> anyhow::Result<()> {
        let affected = self
            .db
            .execute("DELETE FROM devices WHERE id = ?1", &[&device_id])?;
        if affected == 0 {
            anyhow::bail!("device not found: {}", device_id);
        }
        Ok(())
    }

    /// 获取设备
    pub fn get(&self, device_id: &str) -> anyhow::Result<Option<RegisteredDevice>> {
        let mut results = self.db.query_map(
            "SELECT id, name, device_type, ip, port, protocol, paired, paired_at FROM devices WHERE id = ?1",
            &[&device_id],
            |row| {
                Ok(RegisteredDevice {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    device_type: row.get(2)?,
                    ip: row.get(3)?,
                    port: row.get::<_, i32>(4)? as u16,
                    protocol: row.get(5)?,
                    paired: row.get(6)?,
                    paired_at: row.get(7)?,
                })
            },
        )?;
        Ok(results.pop())
    }

    /// 列出所有设备
    pub fn list_all(&self) -> anyhow::Result<Vec<RegisteredDevice>> {
        self.db.query_map(
            "SELECT id, name, device_type, ip, port, protocol, paired, paired_at FROM devices ORDER BY id",
            &[],
            |row| {
                Ok(RegisteredDevice {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    device_type: row.get(2)?,
                    ip: row.get(3)?,
                    port: row.get::<_, i32>(4)? as u16,
                    protocol: row.get(5)?,
                    paired: row.get(6)?,
                    paired_at: row.get(7)?,
                })
            },
        )
    }

    /// 列出已配对设备
    pub fn list_paired(&self) -> anyhow::Result<Vec<RegisteredDevice>> {
        self.db.query_map(
            "SELECT id, name, device_type, ip, port, protocol, paired, paired_at FROM devices WHERE paired = 1 ORDER BY id",
            &[],
            |row| {
                Ok(RegisteredDevice {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    device_type: row.get(2)?,
                    ip: row.get(3)?,
                    port: row.get::<_, i32>(4)? as u16,
                    protocol: row.get(5)?,
                    paired: row.get(6)?,
                    paired_at: row.get(7)?,
                })
            },
        )
    }

    /// 标记设备为已配对
    pub fn mark_paired(&self, device_id: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        let affected = self.db.execute(
            "UPDATE devices SET paired = 1, paired_at = ?1 WHERE id = ?2",
            &[&now, &device_id],
        )?;
        if affected == 0 {
            anyhow::bail!("device not found: {}", device_id);
        }
        Ok(())
    }

    /// 标记设备为未配对
    pub fn mark_unpaired(&self, device_id: &str) -> anyhow::Result<()> {
        let affected = self.db.execute(
            "UPDATE devices SET paired = 0, paired_at = NULL WHERE id = ?1",
            &[&device_id],
        )?;
        if affected == 0 {
            anyhow::bail!("device not found: {}", device_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_registry() -> DeviceRegistry {
        let path = format!("/tmp/prometheus-registry-{}.db", uuid::Uuid::new_v4());
        DeviceRegistry::new(path).unwrap()
    }

    fn sample_device(id: &str) -> RegisteredDevice {
        RegisteredDevice {
            id: id.to_string(),
            name: format!("Device {}", id),
            device_type: "Light".to_string(),
            ip: "192.168.1.10".to_string(),
            port: 8080,
            protocol: "Matter".to_string(),
            paired: false,
            paired_at: None,
        }
    }

    #[test]
    fn register_and_get() {
        let reg = temp_registry();
        let dev = sample_device("light-001");
        reg.register(&dev).unwrap();
        let loaded = reg.get("light-001").unwrap().unwrap();
        assert_eq!(loaded.id, "light-001");
        assert_eq!(loaded.name, "Device light-001");
    }

    #[test]
    fn unregister_removes() {
        let reg = temp_registry();
        reg.register(&sample_device("light-001")).unwrap();
        reg.unregister("light-001").unwrap();
        assert!(reg.get("light-001").unwrap().is_none());
    }

    #[test]
    fn list_all_returns_all() {
        let reg = temp_registry();
        reg.register(&sample_device("a")).unwrap();
        reg.register(&sample_device("b")).unwrap();
        assert_eq!(reg.list_all().unwrap().len(), 2);
    }

    #[test]
    fn mark_paired_and_list_paired() {
        let reg = temp_registry();
        reg.register(&sample_device("light-001")).unwrap();
        reg.register(&sample_device("lock-001")).unwrap();
        reg.mark_paired("light-001").unwrap();
        let paired = reg.list_paired().unwrap();
        assert_eq!(paired.len(), 1);
        assert_eq!(paired[0].id, "light-001");
        assert!(paired[0].paired);
        assert!(paired[0].paired_at.is_some());
    }

    #[test]
    fn mark_unpaired_works() {
        let reg = temp_registry();
        reg.register(&sample_device("light-001")).unwrap();
        reg.mark_paired("light-001").unwrap();
        reg.mark_unpaired("light-001").unwrap();
        let dev = reg.get("light-001").unwrap().unwrap();
        assert!(!dev.paired);
        assert!(dev.paired_at.is_none());
    }

    #[test]
    fn persistence_across_instances() {
        let path = format!("/tmp/prometheus-registry-{}.db", uuid::Uuid::new_v4());
        {
            let reg = DeviceRegistry::new(&path).unwrap();
            reg.register(&sample_device("light-001")).unwrap();
        }
        let reg2 = DeviceRegistry::new(&path).unwrap();
        let dev = reg2.get("light-001").unwrap().unwrap();
        assert_eq!(dev.id, "light-001");
    }
}
