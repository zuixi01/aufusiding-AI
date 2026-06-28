//! BLE (Bluetooth Low Energy) 发现后端
//!
//! 通过 BLE 扫描发现穿戴设备、智能锁、传感器、OBD-II 适配器等。

use async_trait::async_trait;
use std::collections::HashMap;

use super::discovery::DeviceInfo;
use super::discovery_provider::DiscoveryProvider;

/// BLE 服务 UUID 到设备类型的映射
const BLE_SERVICE_MAPPINGS: &[(&str, &str)] = &[
    ("180d", "Wearable"), // Heart Rate Service
    ("1816", "Wearable"), // Cycling Speed and Cadence
    ("181a", "Sensor"),   // Environmental Sensing
    ("1809", "Sensor"),   // Health Thermometer
    ("180f", "Sensor"),   // Battery Service
    ("fff0", "Lock"),     // 常见智能锁自定义服务
    ("fee7", "Wearable"), // 小米手环
    ("ffe0", "Sensor"),   // 通用 BLE 传感器
];

/// BLE 广播数据
#[derive(Debug, Clone)]
pub struct BleAdvertisement {
    pub mac_address: String,
    pub local_name: Option<String>,
    pub service_uuids: Vec<String>,
    pub rssi: i16,
}

/// BLE 扫描器抽象
#[async_trait]
pub trait BleScanner: Send + Sync {
    async fn scan(&self, duration_secs: u64) -> anyhow::Result<Vec<BleAdvertisement>>;
}

/// 默认 BLE 扫描器（返回空结果）
pub struct DefaultBleScanner;

#[async_trait]
impl BleScanner for DefaultBleScanner {
    async fn scan(&self, _duration_secs: u64) -> anyhow::Result<Vec<BleAdvertisement>> {
        Ok(Vec::new())
    }
}

/// Real BLE scanner that uses `bluetoothctl devices` subprocess.
///
/// Falls back to empty results if bluetooth is not available.
pub struct SystemBleScanner;

impl SystemBleScanner {
    pub fn new() -> Self {
        Self
    }

    /// Parse a single line of `bluetoothctl devices` output.
    ///
    /// Expected format: `Device AA:BB:CC:DD:EE:FF Device Name`
    fn parse_device_line(line: &str) -> Option<BleAdvertisement> {
        let line = line.trim();
        if !line.starts_with("Device ") {
            return None;
        }
        let rest = &line["Device ".len()..];
        // MAC address is 17 chars: XX:XX:XX:XX:XX:XX
        if rest.len() < 17 {
            return None;
        }
        let mac = &rest[..17];
        // Validate MAC format
        if mac.len() != 17 || mac.chars().filter(|c| *c == ':').count() != 5 {
            return None;
        }
        let name = rest[17..].trim();
        let local_name = if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        };

        Some(BleAdvertisement {
            mac_address: mac.to_string(),
            local_name,
            service_uuids: Vec::new(),
            rssi: 0,
        })
    }
}

#[async_trait]
impl BleScanner for SystemBleScanner {
    async fn scan(&self, _duration_secs: u64) -> anyhow::Result<Vec<BleAdvertisement>> {
        let output = match tokio::process::Command::new("bluetoothctl")
            .arg("devices")
            .output()
            .await
        {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("BLE: failed to run bluetoothctl: {}", e);
                return Ok(Vec::new());
            }
        };

        if !output.status.success() {
            tracing::warn!(
                "BLE: bluetoothctl exited with status {}",
                output.status
            );
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let devices: Vec<BleAdvertisement> = stdout
            .lines()
            .filter_map(Self::parse_device_line)
            .collect();

        Ok(devices)
    }
}

/// BLE 发现后端
pub struct BleDiscoveryProvider {
    scanner: Box<dyn BleScanner>,
}

impl BleDiscoveryProvider {
    pub fn new() -> Self {
        Self {
            scanner: Box::new(DefaultBleScanner),
        }
    }

    pub fn with_scanner(scanner: Box<dyn BleScanner>) -> Self {
        Self { scanner }
    }

    /// 根据 BLE 服务 UUID 推断设备类型
    fn infer_from_service_uuid(uuid: &str) -> &str {
        let uuid_lower = uuid.to_lowercase();
        for (svc_uuid, dtype) in BLE_SERVICE_MAPPINGS {
            if uuid_lower.contains(svc_uuid) {
                return dtype;
            }
        }
        "Sensor"
    }
}

#[async_trait]
impl DiscoveryProvider for BleDiscoveryProvider {
    fn name(&self) -> &str {
        "BLE"
    }

    async fn scan(&self, timeout_secs: u64) -> anyhow::Result<Vec<DeviceInfo>> {
        let advertisements = self.scanner.scan(timeout_secs).await?;

        // 转换为 DeviceInfo 并按 MAC 地址去重
        let mut seen = HashMap::new();
        let mut devices = Vec::new();

        for ad in advertisements {
            if seen.contains_key(&ad.mac_address) {
                continue;
            }
            seen.insert(ad.mac_address.clone(), true);

            // 从服务 UUID 推断设备类型
            let device_type = if !ad.service_uuids.is_empty() {
                Self::infer_from_service_uuid(&ad.service_uuids[0])
            } else {
                "Sensor"
            };

            let name = ad
                .local_name
                .clone()
                .unwrap_or_else(|| format!("BLE Device {}", &ad.mac_address[..8]));

            devices.push(DeviceInfo {
                id: format!("ble-{}", uuid::Uuid::new_v4()),
                name,
                device_type: device_type.to_string(),
                ip: "0.0.0.0".to_string(), // BLE 设备没有 IP
                port: 0,
                protocol: "ble".to_string(),
            });
        }

        Ok(devices)
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBleScanner {
        advertisements: Vec<BleAdvertisement>,
    }

    #[async_trait]
    impl BleScanner for MockBleScanner {
        async fn scan(&self, _duration_secs: u64) -> anyhow::Result<Vec<BleAdvertisement>> {
            Ok(self.advertisements.clone())
        }
    }

    #[test]
    fn infer_from_service_uuid_all() {
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("0000180d-0000-1000-8000-00805f9b34fb"),
            "Wearable"
        );
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("0000181a-0000-1000-8000-00805f9b34fb"),
            "Sensor"
        );
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("0000fff0-0000-1000-8000-00805f9b34fb"),
            "Lock"
        );
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("0000fee7-0000-1000-8000-00805f9b34fb"),
            "Wearable"
        );
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("0000ffe0-0000-1000-8000-00805f9b34fb"),
            "Sensor"
        );
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("00001816-0000-1000-8000-00805f9b34fb"),
            "Wearable"
        );
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("00001809-0000-1000-8000-00805f9b34fb"),
            "Sensor"
        );
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("0000180f-0000-1000-8000-00805f9b34fb"),
            "Sensor"
        );
        assert_eq!(
            BleDiscoveryProvider::infer_from_service_uuid("00001234-0000-1000-8000-00805f9b34fb"),
            "Sensor"
        );
    }

    #[test]
    fn parse_advertisement() {
        let ad = BleAdvertisement {
            mac_address: "AA:BB:CC:DD:EE:FF".to_string(),
            local_name: Some("Heart Rate Monitor".to_string()),
            service_uuids: vec!["0000180d-0000-1000-8000-00805f9b34fb".to_string()],
            rssi: -60,
        };

        let device_type = BleDiscoveryProvider::infer_from_service_uuid(&ad.service_uuids[0]);
        assert_eq!(device_type, "Wearable");
    }

    #[tokio::test]
    async fn scan_with_mock_returns_devices() {
        let mock = MockBleScanner {
            advertisements: vec![
                BleAdvertisement {
                    mac_address: "AA:BB:CC:DD:EE:01".to_string(),
                    local_name: Some("Heart Rate Monitor".to_string()),
                    service_uuids: vec!["0000180d-0000-1000-8000-00805f9b34fb".to_string()],
                    rssi: -60,
                },
                BleAdvertisement {
                    mac_address: "AA:BB:CC:DD:EE:02".to_string(),
                    local_name: Some("Smart Lock".to_string()),
                    service_uuids: vec!["0000fff0-0000-1000-8000-00805f9b34fb".to_string()],
                    rssi: -70,
                },
                BleAdvertisement {
                    mac_address: "AA:BB:CC:DD:EE:03".to_string(),
                    local_name: Some("Temperature Sensor".to_string()),
                    service_uuids: vec!["0000181a-0000-1000-8000-00805f9b34fb".to_string()],
                    rssi: -55,
                },
            ],
        };

        let provider = BleDiscoveryProvider::with_scanner(Box::new(mock));
        let devices = provider.scan(5).await.unwrap();

        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].device_type, "Wearable");
        assert_eq!(devices[0].name, "Heart Rate Monitor");
        assert_eq!(devices[1].device_type, "Lock");
        assert_eq!(devices[1].name, "Smart Lock");
        assert_eq!(devices[2].device_type, "Sensor");
        assert_eq!(devices[2].name, "Temperature Sensor");
    }

    #[tokio::test]
    async fn scan_empty() {
        let mock = MockBleScanner {
            advertisements: Vec::new(),
        };

        let provider = BleDiscoveryProvider::with_scanner(Box::new(mock));
        let devices = provider.scan(5).await.unwrap();

        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn scan_dedup_same_mac() {
        let mock = MockBleScanner {
            advertisements: vec![
                BleAdvertisement {
                    mac_address: "AA:BB:CC:DD:EE:FF".to_string(),
                    local_name: Some("Device 1".to_string()),
                    service_uuids: vec!["0000180d-0000-1000-8000-00805f9b34fb".to_string()],
                    rssi: -60,
                },
                BleAdvertisement {
                    mac_address: "AA:BB:CC:DD:EE:FF".to_string(),
                    local_name: Some("Device 2".to_string()),
                    service_uuids: vec!["0000180d-0000-1000-8000-00805f9b34fb".to_string()],
                    rssi: -65,
                },
            ],
        };

        let provider = BleDiscoveryProvider::with_scanner(Box::new(mock));
        let devices = provider.scan(5).await.unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Device 1");
    }

    #[test]
    fn system_ble_parse_device_line_valid() {
        let line = "Device AA:BB:CC:DD:EE:FF My Smart Lock";
        let ad = SystemBleScanner::parse_device_line(line).unwrap();
        assert_eq!(ad.mac_address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(ad.local_name.as_deref(), Some("My Smart Lock"));
    }

    #[test]
    fn system_ble_parse_device_line_no_name() {
        let line = "Device AA:BB:CC:DD:EE:FF";
        let ad = SystemBleScanner::parse_device_line(line).unwrap();
        assert_eq!(ad.mac_address, "AA:BB:CC:DD:EE:FF");
        assert!(ad.local_name.is_none());
    }

    #[test]
    fn system_ble_parse_device_line_invalid() {
        assert!(SystemBleScanner::parse_device_line("Not a device line").is_none());
        assert!(SystemBleScanner::parse_device_line("Device short").is_none());
        assert!(SystemBleScanner::parse_device_line("").is_none());
    }
}
