use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrustState {
    Pending,
    Trusted,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DeviceCapabilities {
    pub profile_sync: bool,
    pub session_sync: bool,
    pub settings_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LastSeenMetadata {
    pub ip_address: Option<String>,
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRecord {
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub trust_state: TrustState,
    pub is_local: bool,
    pub capabilities: DeviceCapabilities,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub last_seen_metadata: LastSeenMetadata,
}

impl DeviceRecord {
    pub fn new(device_id: &str, device_name: &str, platform: &str) -> Self {
        Self {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            platform: platform.to_string(),
            trust_state: TrustState::Pending,
            is_local: false,
            capabilities: DeviceCapabilities::default(),
            last_seen_at: None,
            last_seen_metadata: LastSeenMetadata::default(),
        }
    }
}

pub struct DeviceIdentityRegistry {
    local_device_id: String,
    devices: HashMap<String, DeviceRecord>,
}

impl DeviceIdentityRegistry {
    pub fn new(local_device_id: impl Into<String>) -> Self {
        let local_device_id = local_device_id.into();
        Self {
            local_device_id,
            devices: HashMap::new(),
        }
    }

    pub fn register(&mut self, mut record: DeviceRecord) -> anyhow::Result<()> {
        if record.device_id.trim().is_empty() {
            anyhow::bail!("device_id cannot be empty");
        }
        if record.device_name.trim().is_empty() {
            anyhow::bail!("device_name cannot be empty");
        }
        if record.platform.trim().is_empty() {
            anyhow::bail!("platform cannot be empty");
        }

        let is_local_device = record.device_id == self.local_device_id;
        record.is_local = record.is_local || is_local_device;

        if let Some(existing) = self.devices.get(&record.device_id) {
            record.trust_state = existing.trust_state.clone();
            record.last_seen_at = existing.last_seen_at;
            record.last_seen_metadata = existing.last_seen_metadata.clone();
            record.is_local = record.is_local || existing.is_local;
        }

        self.devices.insert(record.device_id.clone(), record);
        Ok(())
    }

    pub fn get(&self, device_id: &str) -> Option<&DeviceRecord> {
        self.devices.get(device_id)
    }

    pub fn list(&self) -> Vec<&DeviceRecord> {
        let mut devices = self.devices.values().collect::<Vec<_>>();
        devices.sort_by(|left, right| left.device_id.cmp(&right.device_id));
        devices
    }

    pub fn set_trust_state(
        &mut self,
        device_id: &str,
        trust_state: TrustState,
    ) -> anyhow::Result<()> {
        let record = self.get_mut(device_id)?;
        record.trust_state = trust_state;
        Ok(())
    }

    pub fn update_last_seen(
        &mut self,
        device_id: &str,
        seen_at: DateTime<Utc>,
        metadata: LastSeenMetadata,
    ) -> anyhow::Result<()> {
        let record = self.get_mut(device_id)?;
        let should_update = record
            .last_seen_at
            .map(|current| seen_at >= current)
            .unwrap_or(true);
        if should_update {
            record.last_seen_at = Some(seen_at);
            record.last_seen_metadata = metadata;
        }
        Ok(())
    }

    pub fn update_capabilities(
        &mut self,
        device_id: &str,
        capabilities: DeviceCapabilities,
    ) -> anyhow::Result<()> {
        let record = self.get_mut(device_id)?;
        record.capabilities = capabilities;
        Ok(())
    }

    pub fn local_device_id(&self) -> &str {
        &self.local_device_id
    }

    fn get_mut(&mut self, device_id: &str) -> anyhow::Result<&mut DeviceRecord> {
        self.devices
            .get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("device not found: {device_id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn remote_device(id: &str) -> DeviceRecord {
        DeviceRecord::new(id, "Alex Phone", "ios")
    }

    #[test]
    fn register_deduplicates_records_by_device_id_and_preserves_local_flag() {
        let mut registry = DeviceIdentityRegistry::new("desktop-1");
        let mut local = DeviceRecord::new("desktop-1", "Desk", "windows");
        local.capabilities = DeviceCapabilities {
            profile_sync: true,
            session_sync: true,
            settings_sync: true,
        };
        let remote = remote_device("phone-1");

        registry.register(local).unwrap();
        registry.register(remote.clone()).unwrap();

        let mut updated_remote = remote.clone();
        updated_remote.device_name = "Alex iPhone".to_string();
        registry.register(updated_remote).unwrap();

        assert_eq!(registry.list().len(), 2);
        assert!(registry.get("desktop-1").unwrap().is_local);
        assert_eq!(registry.get("phone-1").unwrap().device_name, "Alex iPhone");
    }

    #[test]
    fn set_trust_state_updates_existing_device() {
        let mut registry = DeviceIdentityRegistry::new("desktop-1");
        registry.register(remote_device("tablet-1")).unwrap();

        registry
            .set_trust_state("tablet-1", TrustState::Trusted)
            .unwrap();

        assert_eq!(
            registry.get("tablet-1").unwrap().trust_state,
            TrustState::Trusted
        );
    }

    #[test]
    fn update_last_seen_keeps_newest_timestamp_and_metadata() {
        let mut registry = DeviceIdentityRegistry::new("desktop-1");
        registry.register(remote_device("phone-2")).unwrap();
        let newer = Utc::now();
        let older = newer - Duration::minutes(10);

        registry
            .update_last_seen(
                "phone-2",
                newer,
                LastSeenMetadata {
                    ip_address: Some("192.168.1.2".to_string()),
                    app_version: Some("1.0.0".to_string()),
                },
            )
            .unwrap();
        registry
            .update_last_seen(
                "phone-2",
                older,
                LastSeenMetadata {
                    ip_address: Some("10.0.0.8".to_string()),
                    app_version: Some("0.9.0".to_string()),
                },
            )
            .unwrap();

        let record = registry.get("phone-2").unwrap();
        assert_eq!(record.last_seen_at, Some(newer));
        assert_eq!(
            record.last_seen_metadata.ip_address.as_deref(),
            Some("192.168.1.2")
        );
        assert_eq!(
            record.last_seen_metadata.app_version.as_deref(),
            Some("1.0.0")
        );
    }

    #[test]
    fn update_capabilities_replaces_device_flags() {
        let mut registry = DeviceIdentityRegistry::new("desktop-1");
        registry.register(remote_device("laptop-2")).unwrap();

        registry
            .update_capabilities(
                "laptop-2",
                DeviceCapabilities {
                    profile_sync: true,
                    session_sync: false,
                    settings_sync: true,
                },
            )
            .unwrap();

        let capabilities = &registry.get("laptop-2").unwrap().capabilities;
        assert!(capabilities.profile_sync);
        assert!(!capabilities.session_sync);
        assert!(capabilities.settings_sync);
    }
}
