//! 自动配置器
//!
//! 将发现的设备自动注册到 Registry 并关联到正确的适配器。

use std::sync::Arc;

use super::discovery::DeviceInfo;
use super::event_bus::{DeviceEvent, EventBus, EventKind};
use super::registry::{DeviceRegistry, RegisteredDevice};
use super::types::DeviceType;

/// 协议到适配器类型的映射
const PROTOCOL_ADAPTER_MAP: &[(&str, &str)] = &[
    ("ipp", "Printer"),
    ("ipps", "Printer"),
    ("onvif", "Camera"),
    ("rtsp", "Camera"),
    ("homekit", "HomeKit"),
    ("hap", "HomeKit"),
    ("matter", "Matter"),
    ("mqtt", "MQTT"),
    ("http", "HTTP"),
    ("https", "HTTP"),
    ("smb", "FileShare"),
    ("nfs", "FileShare"),
    ("webdav", "FileShare"),
    ("dlna", "DLNA"),
    ("upnp", "UPnP"),
    ("ssdp", "UPnP"),
    ("ble", "BLE"),
    ("bluetooth", "BLE"),
    ("zigbee", "Zigbee"),
    ("zwave", "ZWave"),
    ("tuya", "Tuya"),
    ("mihome", "MiHome"),
    ("midea", "Midea"),
    ("haier", "Haier"),
    ("google_home", "GoogleHome"),
    ("homeassistant", "HomeAssistant"),
    ("obd2", "OBD"),
    ("tesla_api", "Tesla"),
    ("smartcar_api", "SmartCar"),
    ("mdns", "Generic"),
    ("wol", "WoL"),
];

/// 自动配置器 — 发现设备后自动注册并关联适配器
pub struct AutoProvisioner {
    registry: Arc<DeviceRegistry>,
    event_bus: Arc<EventBus>,
}
impl AutoProvisioner {
    pub fn new(registry: Arc<DeviceRegistry>, event_bus: Arc<EventBus>) -> Self {
        Self {
            registry,
            event_bus,
        }
    }

    /// 根据协议推断适配器类型
    pub fn infer_adapter(protocol: &str) -> &'static str {
        let lower = protocol.to_lowercase();
        for (proto, adapter) in PROTOCOL_ADAPTER_MAP {
            if lower == *proto {
                return adapter;
            }
        }
        "Generic"
    }

    /// 手动注册发现的设备
    pub fn provision(&self, device: &DeviceInfo) -> anyhow::Result<()> {
        let registered = RegisteredDevice {
            id: device.id.clone(),
            name: device.name.clone(),
            device_type: device.device_type.clone(),
            ip: device.ip.clone(),
            port: device.port,
            protocol: device.protocol.clone(),
            paired: false,
            paired_at: None,
        };

        self.registry.register(&registered)?;

        let dt = DeviceType::from(device.device_type.as_str());
        self.event_bus.publish(DeviceEvent::new(
            &device.id,
            dt,
            EventKind::DevicePaired,
            serde_json::json!({
                "name": device.name,
                "adapter": Self::infer_adapter(&device.protocol),
            }),
        ));

        Ok(())
    }

    /// 监听 DeviceDiscovered 事件，自动注册
    pub async fn start(&self) {
        let mut rx = self.event_bus.subscribe();
        let registry = Arc::clone(&self.registry);
        let event_bus = Arc::clone(&self.event_bus);

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if event.event_kind != EventKind::DeviceDiscovered {
                    continue;
                }
                let name = event.payload.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let ip = event.payload.get("ip")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0.0")
                    .to_string();
                let port = event.payload.get("port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u16;
                let protocol = event.payload.get("protocol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let device = DeviceInfo {
                    id: event.device_id.clone(),
                    name,
                    device_type: event.device_type.to_string(),
                    ip,
                    port,
                    protocol: protocol.clone(),
                };

                let registered = RegisteredDevice {
                    id: device.id.clone(),
                    name: device.name.clone(),
                    device_type: device.device_type.clone(),
                    ip: device.ip.clone(),
                    port: device.port,
                    protocol: device.protocol.clone(),
                    paired: false,
                    paired_at: None,
                };

                if registry.register(&registered).is_ok() {
                    let dt = DeviceType::from(device.device_type.as_str());
                    event_bus.publish(DeviceEvent::new(
                        &device.id,
                        dt,
                        EventKind::DevicePaired,
                        serde_json::json!({
                            "name": device.name,
                            "adapter": Self::infer_adapter(&protocol),
                        }),
                    ));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_registry() -> Arc<DeviceRegistry> {
        let path = format!("/tmp/prometheus-autoprov-{}.db", uuid::Uuid::new_v4());
        Arc::new(DeviceRegistry::new(&path).unwrap())
    }

    fn sample_device(id: &str, protocol: &str) -> DeviceInfo {
        DeviceInfo {
            id: id.to_string(),
            name: format!("Device {}", id),
            device_type: "Sensor".to_string(),
            ip: "192.168.1.100".to_string(),
            port: 8080,
            protocol: protocol.to_string(),
        }
    }

    #[test]
    fn infer_adapter_ipp() {
        assert_eq!(AutoProvisioner::infer_adapter("ipp"), "Printer");
    }

    #[test]
    fn infer_adapter_onvif() {
        assert_eq!(AutoProvisioner::infer_adapter("onvif"), "Camera");
    }

    #[test]
    fn infer_adapter_homekit() {
        assert_eq!(AutoProvisioner::infer_adapter("homekit"), "HomeKit");
    }

    #[test]
    fn infer_adapter_smb() {
        assert_eq!(AutoProvisioner::infer_adapter("smb"), "FileShare");
    }

    #[test]
    fn infer_adapter_unknown() {
        assert_eq!(AutoProvisioner::infer_adapter("unknown_proto"), "Generic");
    }

    #[test]
    fn provision_registers_device() {
        let registry = temp_registry();
        let bus = Arc::new(EventBus::new(16));
        let provisioner = AutoProvisioner::new(Arc::clone(&registry), bus);

        let device = sample_device("sensor-001", "mqtt");
        provisioner.provision(&device).unwrap();

        let registered = registry.get("sensor-001").unwrap().unwrap();
        assert_eq!(registered.id, "sensor-001");
        assert_eq!(registered.protocol, "mqtt");
    }

    #[test]
    fn provision_duplicate_no_error() {
        let registry = temp_registry();
        let bus = Arc::new(EventBus::new(16));
        let provisioner = AutoProvisioner::new(Arc::clone(&registry), bus);

        let device = sample_device("sensor-001", "mqtt");
        provisioner.provision(&device).unwrap();
        // INSERT OR REPLACE — no error on duplicate
        provisioner.provision(&device).unwrap();
    }

    #[test]
    fn provision_publishes_paired_event() {
        let registry = temp_registry();
        let bus = Arc::new(EventBus::new(16));
        let mut rx = bus.subscribe();
        let provisioner = AutoProvisioner::new(Arc::clone(&registry), bus);

        let device = sample_device("sensor-001", "mqtt");
        provisioner.provision(&device).unwrap();

        let event = rx.try_recv().unwrap();
        assert_eq!(event.event_kind, EventKind::DevicePaired);
        assert_eq!(event.device_id, "sensor-001");
    }

    #[test]
    fn protocol_adapter_map_covers_adapters() {
        let expected = vec![
            "ipp", "onvif", "homekit", "matter", "mqtt", "http", "smb",
            "dlna", "ble", "zigbee", "zwave", "tuya", "mihome", "midea",
            "haier", "google_home", "homeassistant", "obd2", "tesla_api",
            "smartcar_api",
        ];
        for proto in expected {
            let adapter = AutoProvisioner::infer_adapter(proto);
            assert_ne!(adapter, "Generic", "protocol '{}' should have a specific adapter", proto);
        }
    }

    #[test]
    fn full_flow_discover_provision_query() {
        let registry = temp_registry();
        let bus = Arc::new(EventBus::new(16));
        let provisioner = AutoProvisioner::new(Arc::clone(&registry), bus);

        let devices = vec![
            sample_device("printer-001", "ipp"),
            sample_device("camera-001", "onvif"),
            sample_device("speaker-001", "dlna"),
        ];

        for device in &devices {
            provisioner.provision(device).unwrap();
        }

        let all = registry.list_all().unwrap();
        assert_eq!(all.len(), 3);

        let printer = registry.get("printer-001").unwrap().unwrap();
        assert_eq!(printer.protocol, "ipp");
    }
}
