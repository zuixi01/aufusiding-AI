//! 可插拔设备发现后端 trait
//!
//! 定义统一的发现接口，允许 mDNS、SSDP、BLE 等多种后端并行扫描。

use async_trait::async_trait;

use super::discovery::DeviceInfo;

/// 可插拔发现后端
#[async_trait]
pub trait DiscoveryProvider: Send + Sync {
    /// 后端名称（如 "mDNS", "SSDP", "BLE"）
    fn name(&self) -> &str;

    /// 扫描设备，timeout_secs 为超时秒数
    async fn scan(&self, timeout_secs: u64) -> anyhow::Result<Vec<DeviceInfo>>;

    /// 当前平台是否可用
    fn is_available(&self) -> bool {
        true
    }
}

/// 将现有 DeviceDiscovery 适配为 DiscoveryProvider
pub struct MockDiscoveryProvider {
    _discovery: super::discovery::DeviceDiscovery,
}

impl MockDiscoveryProvider {
    pub fn new() -> Self {
        Self {
            _discovery: super::discovery::DeviceDiscovery::new(),
        }
    }
}

#[async_trait]
impl DiscoveryProvider for MockDiscoveryProvider {
    fn name(&self) -> &str {
        "Mock"
    }

    async fn scan(&self, timeout_secs: u64) -> anyhow::Result<Vec<DeviceInfo>> {
        // MockDiscoveryProvider 需要 &mut self 来调用 scan，
        // 但 trait 是 &self。这里创建新实例模拟。
        let mut disc = super::discovery::DeviceDiscovery::new();
        disc.scan(timeout_secs).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_provider_scans() {
        let provider = MockDiscoveryProvider::new();
        assert_eq!(provider.name(), "Mock");
        assert!(provider.is_available());
        let devices = provider.scan(1).await.unwrap();
        assert!(!devices.is_empty());
    }
}
