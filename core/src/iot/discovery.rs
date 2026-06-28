//! 设备发现
//!
//! 使用 mDNS/SSDP 协议自动发现局域网内的智能设备。

use serde::{Deserialize, Serialize};

/// 设备信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceInfo {
    /// 设备 ID
    pub id: String,
    /// 设备名称
    pub name: String,
    /// 设备类型（灯、开关、传感器等）
    pub device_type: String,
    /// IP 地址
    pub ip: String,
    /// 端口
    pub port: u16,
    /// 协议（HomeKit、Matter、自定义等）
    pub protocol: String,
}

/// 设备发现服务
pub struct DeviceDiscovery {
    /// 已发现设备列表
    _devices: Vec<DeviceInfo>,
    /// 是否正在扫描
    scanning: bool,
    /// 模拟设备模板
    mock_templates: Vec<DeviceInfo>,
    /// 可选的聚合器委托
    aggregator: Option<super::discovery_aggregator::DiscoveryAggregator>,
}

impl DeviceDiscovery {
    /// 创建设备发现服务
    pub fn new() -> Self {
        Self {
            _devices: Vec::new(),
            scanning: false,
            mock_templates: default_mock_devices(),
            aggregator: None,
        }
    }

    /// 使用预加载设备创建
    pub fn with_mock_devices(devices: Vec<DeviceInfo>) -> Self {
        Self {
            _devices: devices,
            scanning: false,
            mock_templates: default_mock_devices(),
            aggregator: None,
        }
    }

    /// 设置聚合器委托
    pub fn with_aggregator(mut self, agg: super::discovery_aggregator::DiscoveryAggregator) -> Self {
        self.aggregator = Some(agg);
        self
    }

    /// 开始扫描设备（模拟 mDNS/SSDP）
    pub async fn scan(&mut self, timeout_secs: u64) -> anyhow::Result<Vec<DeviceInfo>> {
        self.scanning = true;

        // 如果有聚合器，委托给它
        if let Some(ref agg) = self.aggregator {
            let found = agg.scan_all(timeout_secs).await?;
            let new_devices: Vec<DeviceInfo> = found
                .into_iter()
                .filter(|d| !self._devices.iter().any(|existing| existing.id == d.id))
                .collect();
            self._devices.extend(new_devices.clone());
            self.scanning = false;
            return Ok(new_devices);
        }

        // 模拟扫描：从模板生成设备，累积到已发现列表
        let new_devices: Vec<DeviceInfo> = self
            .mock_templates
            .iter()
            .filter(|t| !self._devices.iter().any(|d| d.id == t.id))
            .cloned()
            .collect();
        self._devices.extend(new_devices.clone());
        self.scanning = false;
        Ok(new_devices)
    }

    /// 单次扫描（同 scan 但语义更明确）
    pub async fn scan_once(&mut self) -> anyhow::Result<Vec<DeviceInfo>> {
        self.scan(1).await
    }

    /// 停止扫描
    pub fn stop(&mut self) {
        self.scanning = false;
    }

    /// 获取已发现设备
    pub fn devices(&self) -> &[DeviceInfo] {
        &self._devices
    }

    /// 清空已发现设备列表
    pub fn clear(&mut self) {
        self._devices.clear();
    }
}

fn default_mock_devices() -> Vec<DeviceInfo> {
    vec![
        DeviceInfo {
            id: "light-001".into(),
            name: "客厅灯".into(),
            device_type: "Light".into(),
            ip: "192.168.1.10".into(),
            port: 8080,
            protocol: "Matter".into(),
        },
        DeviceInfo {
            id: "thermo-001".into(),
            name: "卧室温控器".into(),
            device_type: "Thermostat".into(),
            ip: "192.168.1.11".into(),
            port: 8080,
            protocol: "HomeKit".into(),
        },
        DeviceInfo {
            id: "camera-001".into(),
            name: "门口摄像头".into(),
            device_type: "Camera".into(),
            ip: "192.168.1.12".into(),
            port: 554,
            protocol: "ONVIF".into(),
        },
        DeviceInfo {
            id: "lock-001".into(),
            name: "前门智能锁".into(),
            device_type: "Lock".into(),
            ip: "192.168.1.13".into(),
            port: 8080,
            protocol: "Matter".into(),
        },
        DeviceInfo {
            id: "sensor-001".into(),
            name: "温湿度传感器".into(),
            device_type: "Sensor".into(),
            ip: "192.168.1.14".into(),
            port: 8080,
            protocol: "Zigbee".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_creates_empty() {
        let d = DeviceDiscovery::new();
        assert!(d.devices().is_empty());
    }

    #[tokio::test]
    async fn scan_discovers_devices() {
        let mut d = DeviceDiscovery::new();
        let found = d.scan(1).await.unwrap();
        assert_eq!(found.len(), 5);
        assert_eq!(d.devices().len(), 5);
    }

    #[tokio::test]
    async fn scan_accumulates_no_duplicates() {
        let mut d = DeviceDiscovery::new();
        d.scan(1).await.unwrap();
        let second = d.scan(1).await.unwrap();
        assert!(second.is_empty());
        assert_eq!(d.devices().len(), 5);
    }

    #[test]
    fn stop_sets_flag() {
        let mut d = DeviceDiscovery::new();
        d.scanning = true;
        d.stop();
        assert!(!d.scanning);
    }

    #[test]
    fn with_mock_devices_preloads() {
        let devices = vec![DeviceInfo {
            id: "test-1".into(),
            name: "Test".into(),
            device_type: "Light".into(),
            ip: "10.0.0.1".into(),
            port: 80,
            protocol: "HTTP".into(),
        }];
        let d = DeviceDiscovery::with_mock_devices(devices);
        assert_eq!(d.devices().len(), 1);
    }

    #[test]
    fn clear_removes_all() {
        let devices = vec![DeviceInfo {
            id: "test-1".into(),
            name: "Test".into(),
            device_type: "Light".into(),
            ip: "10.0.0.1".into(),
            port: 80,
            protocol: "HTTP".into(),
        }];
        let mut d = DeviceDiscovery::with_mock_devices(devices);
        assert_eq!(d.devices().len(), 1);
        d.clear();
        assert!(d.devices().is_empty());
    }

    #[test]
    fn with_aggregator_sets_field() {
        let agg = crate::iot::discovery_aggregator::DiscoveryAggregator::new();
        let d = DeviceDiscovery::new().with_aggregator(agg);
        assert!(d.aggregator.is_some());
    }

    #[tokio::test]
    async fn scan_without_aggregator_uses_templates() {
        let mut d = DeviceDiscovery::new();
        assert!(d.aggregator.is_none());
        let found = d.scan(1).await.unwrap();
        assert_eq!(found.len(), 5);
    }
}
