//! 增强设备信息
//!
//! 包装原有 `DeviceInfo`，添加类型安全字段和能力集。
//! 通过 `From<DeviceInfo>` 桥接，不修改原有结构。

use serde::{Deserialize, Serialize};

use super::discovery::DeviceInfo;
use super::types::{default_capabilities, CapabilitySet, DeviceType, Protocol};

/// 增强设备信息（类型安全 + 能力声明）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedDeviceInfo {
    /// 原始设备信息
    pub base: DeviceInfo,
    /// 类型安全的设备类型
    pub typed_device_type: DeviceType,
    /// 类型安全的协议
    pub typed_protocol: Protocol,
    /// 设备能力集
    pub capabilities: CapabilitySet,
}

impl EnhancedDeviceInfo {
    /// 从原始 DeviceInfo 创建，自动推断类型和能力
    pub fn from_device_info(info: DeviceInfo) -> Self {
        let typed_device_type = DeviceType::from(info.device_type.as_str());
        let typed_protocol = Protocol::from(info.protocol.as_str());
        let capabilities = default_capabilities(&typed_device_type);
        Self {
            base: info,
            typed_device_type,
            typed_protocol,
            capabilities,
        }
    }

    /// 创建并覆盖能力集
    pub fn with_capabilities(mut self, caps: CapabilitySet) -> Self {
        self.capabilities = caps;
        self
    }

    /// 检查设备是否具有某能力
    pub fn has_capability(&self, cap: &super::types::Capability) -> bool {
        self.capabilities.has(cap)
    }

    /// 设备 ID
    pub fn id(&self) -> &str {
        &self.base.id
    }

    /// 设备名称
    pub fn name(&self) -> &str {
        &self.base.name
    }
}

impl From<DeviceInfo> for EnhancedDeviceInfo {
    fn from(info: DeviceInfo) -> Self {
        Self::from_device_info(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::types::Capability;

    fn sample_device_info() -> DeviceInfo {
        DeviceInfo {
            id: "printer-001".to_string(),
            name: "Office Printer".to_string(),
            device_type: "Printer".to_string(),
            ip: "192.168.1.50".to_string(),
            port: 631,
            protocol: "ipp".to_string(),
        }
    }

    #[test]
    fn from_device_info_preserves_base() {
        let info = sample_device_info();
        let enhanced = EnhancedDeviceInfo::from(info.clone());
        assert_eq!(enhanced.base.id, info.id);
        assert_eq!(enhanced.base.ip, info.ip);
        assert_eq!(enhanced.base.port, info.port);
    }

    #[test]
    fn from_device_info_infers_type() {
        let enhanced = EnhancedDeviceInfo::from(sample_device_info());
        assert_eq!(enhanced.typed_device_type, DeviceType::Printer);
        assert_eq!(enhanced.typed_protocol, Protocol::Ipp);
    }

    #[test]
    fn from_device_info_has_capabilities() {
        let enhanced = EnhancedDeviceInfo::from(sample_device_info());
        assert!(enhanced.has_capability(&Capability::Print));
        assert!(enhanced.has_capability(&Capability::PrintStatus));
        assert!(!enhanced.has_capability(&Capability::Switch));
    }

    #[test]
    fn with_capabilities_overrides() {
        let enhanced = EnhancedDeviceInfo::from(sample_device_info())
            .with_capabilities(CapabilitySet::new([Capability::Switch]));
        assert!(enhanced.has_capability(&Capability::Switch));
        assert!(!enhanced.has_capability(&Capability::Print));
    }

    #[test]
    fn id_and_name_accessors() {
        let enhanced = EnhancedDeviceInfo::from(sample_device_info());
        assert_eq!(enhanced.id(), "printer-001");
        assert_eq!(enhanced.name(), "Office Printer");
    }

    #[test]
    fn light_device_info_conversion() {
        let info = DeviceInfo {
            id: "light-001".to_string(),
            name: "Living Room Light".to_string(),
            device_type: "Light".to_string(),
            ip: "192.168.1.10".to_string(),
            port: 8080,
            protocol: "matter".to_string(),
        };
        let enhanced = EnhancedDeviceInfo::from(info);
        assert_eq!(enhanced.typed_device_type, DeviceType::Light);
        assert_eq!(enhanced.typed_protocol, Protocol::Matter);
        assert!(enhanced.has_capability(&Capability::Switch));
        assert!(enhanced.has_capability(&Capability::Dimmer));
    }
}
