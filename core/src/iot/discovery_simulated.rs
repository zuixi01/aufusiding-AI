//! 模拟网络发现后端
//!
//! 为集成测试提供返回真实数据的模拟 socket，模拟网络延迟。

use async_trait::async_trait;
use std::collections::HashMap;

use super::discovery_ble::{BleAdvertisement, BleScanner};
use super::discovery_mdns::{MdnsResponse, MdnsSocket};
use super::discovery_ssdp::{SsdpResponse, SsdpSocket};

// ─── Simulated mDNS ─────────────────────────────────────

pub struct SimulatedMdnsSocket {
    pub devices: Vec<MdnsResponse>,
    pub delay_ms: u64,
}

impl SimulatedMdnsSocket {
    pub fn new(devices: Vec<MdnsResponse>, delay_ms: u64) -> Self {
        Self { devices, delay_ms }
    }

    /// 工厂：典型局域网设备集
    pub fn typical_lan_devices() -> Self {
        Self::new(vec![
            MdnsResponse {
                hostname: "hp-printer.local".into(),
                ip: "192.168.1.10".into(),
                port: 631,
                service_type: "_ipp._tcp.local.".into(),
                txt_records: HashMap::from([("name".into(), "HP LaserJet".into())]),
            },
            MdnsResponse {
                hostname: "sonos-living.local".into(),
                ip: "192.168.1.20".into(),
                port: 1400,
                service_type: "_airplay._tcp.local.".into(),
                txt_records: HashMap::from([("name".into(), "Sonos Living Room".into())]),
            },
            MdnsResponse {
                hostname: "nas-synology.local".into(),
                ip: "192.168.1.30".into(),
                port: 445,
                service_type: "_smb._tcp.local.".into(),
                txt_records: HashMap::from([("name".into(), "Synology NAS".into())]),
            },
            MdnsResponse {
                hostname: "homekit-light.local".into(),
                ip: "192.168.1.40".into(),
                port: 8080,
                service_type: "_hap._tcp.local.".into(),
                txt_records: HashMap::from([("name".into(), "HomeKit Light".into())]),
            },
            MdnsResponse {
                hostname: "chromecast.local".into(),
                ip: "192.168.1.50".into(),
                port: 8008,
                service_type: "_googlecast._tcp.local.".into(),
                txt_records: HashMap::from([("name".into(), "Chromecast Ultra".into())]),
            },
        ], 10)
    }
}

#[async_trait]
impl MdnsSocket for SimulatedMdnsSocket {
    async fn send_query(&self, _service_type: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn recv_responses(&self, _timeout_secs: u64) -> anyhow::Result<Vec<MdnsResponse>> {
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }
        Ok(self.devices.clone())
    }
}

// ─── Simulated SSDP ─────────────────────────────────────

pub struct SimulatedSsdpSocket {
    pub devices: Vec<SsdpResponse>,
    pub delay_ms: u64,
}

impl SimulatedSsdpSocket {
    pub fn new(devices: Vec<SsdpResponse>, delay_ms: u64) -> Self {
        Self { devices, delay_ms }
    }

    /// 工厂：典型 UPnP 设备集
    pub fn typical_upnp_devices() -> Self {
        Self::new(vec![
            SsdpResponse {
                location: "http://192.168.1.60:8200/rootDesc.xml".into(),
                search_target: "urn:schemas-upnp-org:device:MediaRenderer:1".into(),
                usn: "uuid:renderer-001".into(),
                server: "DLNA Speaker".into(),
            },
            SsdpResponse {
                location: "http://192.168.1.70:49152/desc.xml".into(),
                search_target: "urn:schemas-upnp-org:device:MediaServer:1".into(),
                usn: "uuid:server-001".into(),
                server: "Plex Media Server".into(),
            },
            SsdpResponse {
                location: "http://192.168.1.1:80/rootDesc.xml".into(),
                search_target: "urn:schemas-upnp-org:device:InternetGatewayDevice:1".into(),
                usn: "uuid:gateway-001".into(),
                server: "Home Router".into(),
            },
        ], 10)
    }
}

#[async_trait]
impl SsdpSocket for SimulatedSsdpSocket {
    async fn send_msearch(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn recv_responses(&self, _timeout_secs: u64) -> anyhow::Result<Vec<SsdpResponse>> {
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }
        Ok(self.devices.clone())
    }
}

// ─── Simulated BLE ───────────────────────────────────────

pub struct SimulatedBleScanner {
    pub devices: Vec<BleAdvertisement>,
    pub delay_ms: u64,
}

impl SimulatedBleScanner {
    pub fn new(devices: Vec<BleAdvertisement>, delay_ms: u64) -> Self {
        Self { devices, delay_ms }
    }

    /// 工厂：典型 BLE 设备集
    pub fn typical_ble_devices() -> Self {
        Self::new(vec![
            BleAdvertisement {
                mac_address: "AA:BB:CC:DD:EE:01".into(),
                local_name: Some("Mi Band 7".into()),
                service_uuids: vec!["0000fee7-0000-1000-8000-00805f9b34fb".into()],
                rssi: -55,
            },
            BleAdvertisement {
                mac_address: "AA:BB:CC:DD:EE:02".into(),
                local_name: Some("Smart Lock Pro".into()),
                service_uuids: vec!["0000fff0-0000-1000-8000-00805f9b34fb".into()],
                rssi: -40,
            },
        ], 10)
    }
}

#[async_trait]
impl BleScanner for SimulatedBleScanner {
    async fn scan(&self, _duration_secs: u64) -> anyhow::Result<Vec<BleAdvertisement>> {
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }
        Ok(self.devices.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::discovery_ble::BleDiscoveryProvider;
    use crate::iot::discovery_mdns::MdnsDiscoveryProvider;
    use crate::iot::discovery_provider::DiscoveryProvider;
    use crate::iot::discovery_ssdp::SsdpDiscoveryProvider;

    #[tokio::test]
    async fn simulated_mdns_returns_devices() {
        let socket = SimulatedMdnsSocket::typical_lan_devices();
        let provider = MdnsDiscoveryProvider::with_socket(Box::new(socket));
        let devices = provider.scan(1).await.unwrap();
        assert_eq!(devices.len(), 5);
    }

    #[tokio::test]
    async fn simulated_ssdp_returns_devices() {
        let socket = SimulatedSsdpSocket::typical_upnp_devices();
        let provider = SsdpDiscoveryProvider::with_socket(Box::new(socket));
        let devices = provider.scan(1).await.unwrap();
        assert_eq!(devices.len(), 3);
    }

    #[tokio::test]
    async fn simulated_ble_returns_devices() {
        let scanner = SimulatedBleScanner::typical_ble_devices();
        let provider = BleDiscoveryProvider::with_scanner(Box::new(scanner));
        let devices = provider.scan(1).await.unwrap();
        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn simulated_delay() {
        let socket = SimulatedMdnsSocket::new(vec![
            MdnsResponse {
                hostname: "test.local".into(),
                ip: "10.0.0.1".into(),
                port: 80,
                service_type: "_http._tcp.local.".into(),
                txt_records: HashMap::new(),
            },
        ], 50);
        let start = tokio::time::Instant::now();
        let provider = MdnsDiscoveryProvider::with_socket(Box::new(socket));
        let devices = provider.scan(1).await.unwrap();
        let elapsed = start.elapsed().as_millis();
        assert_eq!(devices.len(), 1);
        assert!(elapsed >= 40); // Allow some tolerance
    }

    #[tokio::test]
    async fn simulated_empty_devices() {
        let socket = SimulatedMdnsSocket::new(vec![], 0);
        let provider = MdnsDiscoveryProvider::with_socket(Box::new(socket));
        let devices = provider.scan(1).await.unwrap();
        assert!(devices.is_empty());
    }
}
