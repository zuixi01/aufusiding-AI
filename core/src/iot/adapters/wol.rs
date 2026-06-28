//! Wake-on-LAN adapter
//!
//! Sends magic packets to wake LAN computers via a `UdpTransport` trait abstraction.
//! Capability: WakeOnLan
//! Supports SecureOn password (6-byte suffix).

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// MAC address
// ---------------------------------------------------------------------------

/// MAC address (6 bytes).
#[derive(Debug, Clone)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// Parse from "AA:BB:CC:DD:EE:FF" or "AA-BB-CC-DD-EE-FF" format.
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = s.split([':', '-']).collect();
        if parts.len() != 6 {
            anyhow::bail!("invalid MAC address: {}", s);
        }
        let mut bytes = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            bytes[i] = u8::from_str_radix(part, 16)
                .map_err(|_| anyhow::anyhow!("invalid hex: {}", part))?;
        }
        Ok(Self(bytes))
    }

    /// Build a standard magic packet: 6x 0xFF + 16x MAC (102 bytes).
    pub fn magic_packet(&self) -> Vec<u8> {
        let mut packet = vec![0xFFu8; 6];
        for _ in 0..16 {
            packet.extend_from_slice(&self.0);
        }
        packet
    }

    /// Build a magic packet with SecureOn password (6-byte suffix, 108 bytes).
    pub fn magic_packet_with_password(&self, password: &[u8; 6]) -> Vec<u8> {
        let mut packet = self.magic_packet();
        packet.extend_from_slice(password);
        packet
    }
}

// ---------------------------------------------------------------------------
// UDP transport trait
// ---------------------------------------------------------------------------

/// Abstraction over UDP socket for testability.
#[async_trait]
pub trait UdpTransport: Send + Sync {
    /// Send a packet to the given address.
    async fn send_to(&self, packet: &[u8], addr: &str) -> anyhow::Result<()>;
}

/// Real UDP transport using std::net::UdpSocket.
pub struct RealUdpTransport;

#[async_trait]
impl UdpTransport for RealUdpTransport {
    async fn send_to(&self, packet: &[u8], addr: &str) -> anyhow::Result<()> {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        socket.set_broadcast(true)?;
        socket.send_to(packet, addr)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mock UDP transport
// ---------------------------------------------------------------------------

/// Recorded UDP send.
#[derive(Debug, Clone)]
pub struct UdpSend {
    pub packet: Vec<u8>,
    pub addr: String,
}

/// Mock UDP transport that records sent packets.
pub struct MockUdpTransport {
    pub sends: Arc<RwLock<Vec<UdpSend>>>,
}

impl MockUdpTransport {
    pub fn new() -> Self {
        Self {
            sends: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn sends(&self) -> Vec<UdpSend> {
        self.sends.read().await.clone()
    }
}

#[async_trait]
impl UdpTransport for MockUdpTransport {
    async fn send_to(&self, packet: &[u8], addr: &str) -> anyhow::Result<()> {
        self.sends.write().await.push(UdpSend {
            packet: packet.to_vec(),
            addr: addr.to_string(),
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// WoL device config
// ---------------------------------------------------------------------------

/// WoL device configuration.
#[derive(Debug, Clone)]
pub struct WolDeviceConfig {
    pub mac: MacAddress,
    pub broadcast_addr: String,
    pub secure_on_password: Option<[u8; 6]>,
}

impl WolDeviceConfig {
    /// Create a simple config with default broadcast address.
    pub fn new(mac: MacAddress) -> Self {
        Self {
            mac,
            broadcast_addr: "255.255.255.255:9".to_string(),
            secure_on_password: None,
        }
    }

    /// Set a custom broadcast address.
    pub fn with_broadcast(mut self, addr: &str) -> Self {
        self.broadcast_addr = addr.to_string();
        self
    }

    /// Set a SecureOn password.
    pub fn with_password(mut self, password: [u8; 6]) -> Self {
        self.secure_on_password = Some(password);
        self
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Wake-on-LAN adapter using a pluggable UDP transport.
pub struct WolAdapter {
    devices: HashMap<String, WolDeviceConfig>,
    transport: Arc<dyn UdpTransport>,
}

impl WolAdapter {
    pub fn new(transport: Arc<dyn UdpTransport>) -> Self {
        Self {
            devices: HashMap::new(),
            transport,
        }
    }

    /// Convenience constructor with mock transport.
    pub fn with_mock() -> (Self, Arc<MockUdpTransport>) {
        let mock = Arc::new(MockUdpTransport::new());
        let adapter = Self::new(mock.clone());
        (adapter, mock)
    }

    /// Register a device with just a MAC address (backward compat).
    pub fn register_device(&mut self, device_id: &str, mac: MacAddress) {
        self.devices
            .insert(device_id.to_string(), WolDeviceConfig::new(mac));
    }

    /// Register a device with full config.
    pub fn register_device_config(&mut self, device_id: &str, config: WolDeviceConfig) {
        self.devices.insert(device_id.to_string(), config);
    }

    /// Send a magic packet for the given device.
    async fn send_magic_packet(&self, device_id: &str) -> anyhow::Result<()> {
        let config = self
            .devices
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown device: {}", device_id))?;
        let packet = match &config.secure_on_password {
            Some(pw) => config.mac.magic_packet_with_password(pw),
            None => config.mac.magic_packet(),
        };
        self.transport.send_to(&packet, &config.broadcast_addr).await
    }
}

#[async_trait]
impl DeviceController for WolAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_magic_packet(device_id).await
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("WoL cannot turn off devices -- use SSH/RPC shutdown instead")
    }

    async fn set_property(
        &self,
        _device_id: &str,
        _property: &str,
        _value: Value,
    ) -> anyhow::Result<()> {
        anyhow::bail!("WoL only supports turn_on")
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let has = self.devices.contains_key(device_id);
        Ok(serde_json::json!({
            "registered": has,
            "note": "WoL cannot query device state -- use ping or SSH to check"
        }))
    }

    fn supported_type(&self) -> &str {
        "Computer"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (WolAdapter, Arc<MockUdpTransport>) {
        let (mut adapter, mock) = WolAdapter::with_mock();
        adapter.register_device("pc-001", MacAddress::parse("AA:BB:CC:DD:EE:FF").unwrap());
        (adapter, mock)
    }

    // 1. parse MAC colon format
    #[test]
    fn parse_mac_colon() {
        let mac = MacAddress::parse("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(mac.0, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    // 2. parse MAC dash format
    #[test]
    fn parse_mac_dash() {
        let mac = MacAddress::parse("11-22-33-44-55-66").unwrap();
        assert_eq!(mac.0, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    }

    // 3. parse MAC invalid
    #[test]
    fn parse_mac_invalid() {
        assert!(MacAddress::parse("invalid").is_err());
        assert!(MacAddress::parse("AA:BB:CC").is_err());
        assert!(MacAddress::parse("GG:HH:II:JJ:KK:LL").is_err());
    }

    // 4. magic packet format (102 bytes)
    #[test]
    fn magic_packet_format() {
        let mac = MacAddress::parse("AA:BB:CC:DD:EE:FF").unwrap();
        let packet = mac.magic_packet();
        assert_eq!(packet.len(), 102);
        assert!(packet[..6].iter().all(|&b| b == 0xFF));
        for i in 0..16 {
            let offset = 6 + i * 6;
            assert_eq!(
                &packet[offset..offset + 6],
                &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]
            );
        }
    }

    // 5. magic packet with SecureOn password (108 bytes)
    #[test]
    fn magic_packet_with_password() {
        let mac = MacAddress::parse("AA:BB:CC:DD:EE:FF").unwrap();
        let pw = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let packet = mac.magic_packet_with_password(&pw);
        assert_eq!(packet.len(), 108);
        assert_eq!(&packet[102..108], &pw);
    }

    // 6. supported_type
    #[test]
    fn supported_type_is_computer() {
        let (adapter, _) = WolAdapter::with_mock();
        assert_eq!(adapter.supported_type(), "Computer");
    }

    // 7. turn_on sends magic packet to correct address
    #[tokio::test]
    async fn turn_on_sends_magic_packet() {
        let (adapter, mock) = setup();
        adapter.turn_on("pc-001").await.unwrap();
        let sends = mock.sends().await;
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].addr, "255.255.255.255:9");
        assert_eq!(sends[0].packet.len(), 102);
        assert!(sends[0].packet[..6].iter().all(|&b| b == 0xFF));
    }

    // 8. turn_on with custom broadcast address
    #[tokio::test]
    async fn turn_on_custom_broadcast() {
        let (mut adapter, mock) = WolAdapter::with_mock();
        let config = WolDeviceConfig::new(MacAddress::parse("11:22:33:44:55:66").unwrap())
            .with_broadcast("192.168.1.255:9");
        adapter.register_device_config("pc-002", config);
        adapter.turn_on("pc-002").await.unwrap();
        let sends = mock.sends().await;
        assert_eq!(sends[0].addr, "192.168.1.255:9");
    }

    // 9. turn_on with SecureOn password sends 108-byte packet
    #[tokio::test]
    async fn turn_on_with_secure_on_password() {
        let (mut adapter, mock) = WolAdapter::with_mock();
        let pw = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let config = WolDeviceConfig::new(MacAddress::parse("11:22:33:44:55:66").unwrap())
            .with_password(pw);
        adapter.register_device_config("pc-003", config);
        adapter.turn_on("pc-003").await.unwrap();
        let sends = mock.sends().await;
        assert_eq!(sends[0].packet.len(), 108);
        assert_eq!(&sends[0].packet[102..108], &pw);
    }

    // 10. turn_off not supported
    #[tokio::test]
    async fn turn_off_not_supported() {
        let (adapter, _) = setup();
        assert!(adapter.turn_off("pc-001").await.is_err());
    }

    // 11. set_property not supported
    #[tokio::test]
    async fn set_property_not_supported() {
        let (adapter, _) = setup();
        assert!(adapter
            .set_property("pc-001", "anything", Value::Null)
            .await
            .is_err());
    }

    // 12. unknown device turn_on errors
    #[tokio::test]
    async fn unknown_device_turn_on_errors() {
        let (adapter, _) = WolAdapter::with_mock();
        assert!(adapter.turn_on("unknown").await.is_err());
    }

    // 13. get_state registered device
    #[tokio::test]
    async fn get_state_registered() {
        let (adapter, _) = setup();
        let state = adapter.get_state("pc-001").await.unwrap();
        assert_eq!(state["registered"], true);
    }

    // 14. get_state unregistered device
    #[tokio::test]
    async fn get_state_unregistered() {
        let (adapter, _) = WolAdapter::with_mock();
        let state = adapter.get_state("unknown").await.unwrap();
        assert_eq!(state["registered"], false);
    }

    // 15. magic packet MAC bytes are correct in sent packet
    #[tokio::test]
    async fn magic_packet_mac_bytes_correct() {
        let (adapter, mock) = setup();
        adapter.turn_on("pc-001").await.unwrap();
        let sends = mock.sends().await;
        let packet = &sends[0].packet;
        // Check all 16 repetitions of the MAC
        for i in 0..16 {
            let offset = 6 + i * 6;
            assert_eq!(
                &packet[offset..offset + 6],
                &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]
            );
        }
    }
}
