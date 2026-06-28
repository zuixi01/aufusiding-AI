//! BLE Wearable Device Adapter
//!
//! Supports BLE GATT protocol for smart bands/watches with standard services.
//! Capabilities: HealthMonitoring, StepTracking, HeartRate

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// BLE GATT UUIDs
// ---------------------------------------------------------------------------

/// Standard GATT service UUIDs
pub mod service_uuid {
    pub const HEART_RATE: &str = "0000180d-0000-1000-8000-00805f9b34fb";
    pub const BATTERY: &str = "0000180f-0000-1000-8000-00805f9b34fb";
    pub const DEVICE_INFO: &str = "0000180a-0000-1000-8000-00805f9b34fb";
    pub const CURRENT_TIME: &str = "00001805-0000-1000-8000-00805f9b34fb";
    pub const USER_DATA: &str = "0000181c-0000-1000-8000-00805f9b34fb";
}

/// Standard GATT characteristic UUIDs
pub mod char_uuid {
    pub const HEART_RATE_MEASUREMENT: &str = "00002a37-0000-1000-8000-00805f9b34fb";
    pub const BATTERY_LEVEL: &str = "00002a19-0000-1000-8000-00805f9b34fb";
    pub const MODEL_NUMBER: &str = "00002a24-0000-1000-8000-00805f9b34fb";
    pub const FIRMWARE_REVISION: &str = "00002a26-0000-1000-8000-00805f9b34fb";
    pub const MANUFACTURER_NAME: &str = "00002a29-0000-1000-8000-00805f9b34fb";
    pub const CURRENT_TIME: &str = "00002a2b-0000-1000-8000-00805f9b34fb";
    pub const STEP_COUNT: &str = "00002a2c-0000-1000-8000-00805f9b34fb";
}

// ---------------------------------------------------------------------------
// BLE Transport Trait
// ---------------------------------------------------------------------------

/// BLE service descriptor
#[derive(Debug, Clone)]
pub struct BleService {
    pub uuid: String,
    pub characteristics: Vec<String>,
}

/// Abstraction over BLE GATT operations for testability
#[async_trait]
pub trait BleTransport: Send + Sync {
    /// Connect to a BLE device by MAC address
    async fn connect(&self, addr: &str) -> anyhow::Result<()>;

    /// Disconnect from the current device
    async fn disconnect(&self) -> anyhow::Result<()>;

    /// Discover available services
    async fn discover_services(&self) -> anyhow::Result<Vec<BleService>>;

    /// Read a characteristic value
    async fn read_characteristic(
        &self,
        service_uuid: &str,
        char_uuid: &str,
    ) -> anyhow::Result<Vec<u8>>;

    /// Write a characteristic value
    async fn write_characteristic(
        &self,
        service_uuid: &str,
        char_uuid: &str,
        data: &[u8],
    ) -> anyhow::Result<()>;
}

// ---------------------------------------------------------------------------
// Mock BLE Transport
// ---------------------------------------------------------------------------

/// Recorded BLE operation
#[derive(Debug, Clone)]
pub enum BleOperation {
    Connect { addr: String },
    Disconnect,
    DiscoverServices,
    ReadCharacteristic { service_uuid: String, char_uuid: String },
    WriteCharacteristic { service_uuid: String, char_uuid: String, data: Vec<u8> },
}

/// Mock BLE transport that records operations and returns canned responses
pub struct MockBleTransport {
    pub operations: Arc<RwLock<Vec<BleOperation>>>,
    pub responses: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    pub connected: Arc<RwLock<bool>>,
}

impl MockBleTransport {
    pub fn new() -> Self {
        Self {
            operations: Arc::new(RwLock::new(Vec::new())),
            responses: Arc::new(RwLock::new(HashMap::new())),
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Set a canned response for a characteristic read
    pub async fn set_response(&self, service_uuid: &str, char_uuid: &str, data: Vec<u8>) {
        let key = format!("{}:{}", service_uuid, char_uuid);
        self.responses.write().await.insert(key, data);
    }

    /// Get recorded operations
    pub async fn operations(&self) -> Vec<BleOperation> {
        self.operations.read().await.clone()
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
}

#[async_trait]
impl BleTransport for MockBleTransport {
    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        self.operations.write().await.push(BleOperation::Connect {
            addr: addr.to_string(),
        });
        *self.connected.write().await = true;
        Ok(())
    }

    async fn disconnect(&self) -> anyhow::Result<()> {
        self.operations.write().await.push(BleOperation::Disconnect);
        *self.connected.write().await = false;
        Ok(())
    }

    async fn discover_services(&self) -> anyhow::Result<Vec<BleService>> {
        self.operations.write().await.push(BleOperation::DiscoverServices);
        Ok(vec![
            BleService {
                uuid: service_uuid::HEART_RATE.to_string(),
                characteristics: vec![char_uuid::HEART_RATE_MEASUREMENT.to_string()],
            },
            BleService {
                uuid: service_uuid::BATTERY.to_string(),
                characteristics: vec![char_uuid::BATTERY_LEVEL.to_string()],
            },
        ])
    }

    async fn read_characteristic(
        &self,
        service_uuid: &str,
        char_uuid: &str,
    ) -> anyhow::Result<Vec<u8>> {
        self.operations.write().await.push(BleOperation::ReadCharacteristic {
            service_uuid: service_uuid.to_string(),
            char_uuid: char_uuid.to_string(),
        });

        let key = format!("{}:{}", service_uuid, char_uuid);
        let responses = self.responses.read().await;
        responses
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no mock response for {}", key))
    }

    async fn write_characteristic(
        &self,
        service_uuid: &str,
        char_uuid: &str,
        data: &[u8],
    ) -> anyhow::Result<()> {
        self.operations.write().await.push(BleOperation::WriteCharacteristic {
            service_uuid: service_uuid.to_string(),
            char_uuid: char_uuid.to_string(),
            data: data.to_vec(),
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// GATT Data Parsing
// ---------------------------------------------------------------------------

/// Parsed heart rate measurement
#[derive(Debug, Clone, PartialEq)]
pub struct HeartRateMeasurement {
    pub bpm: u16,
    pub rr_intervals: Vec<u16>,
}

/// Parse heart rate measurement characteristic (0x2A37)
///
/// Byte 0: Flags
///   - bit 0: 0=uint8 HR, 1=uint16 HR
///   - bit 4: 1=RR intervals present
///
/// Bytes 1+: HR value (uint8 or uint16)
///
/// Bytes after HR: RR intervals (uint16 each) if present
pub fn parse_heart_rate(data: &[u8]) -> anyhow::Result<HeartRateMeasurement> {
    if data.is_empty() {
        anyhow::bail!("empty heart rate data");
    }

    let flags = data[0];
    let hr_is_u16 = (flags & 0x01) != 0;
    let rr_present = (flags & 0x10) != 0;

    let (bpm, offset) = if hr_is_u16 {
        if data.len() < 3 {
            anyhow::bail!("insufficient data for uint16 heart rate");
        }
        let bpm = u16::from_le_bytes([data[1], data[2]]);
        (bpm, 3)
    } else {
        if data.len() < 2 {
            anyhow::bail!("insufficient data for uint8 heart rate");
        }
        (data[1] as u16, 2)
    };

    let mut rr_intervals = Vec::new();
    if rr_present {
        let mut pos = offset;
        while pos + 1 < data.len() {
            let rr = u16::from_le_bytes([data[pos], data[pos + 1]]);
            rr_intervals.push(rr);
            pos += 2;
        }
    }

    Ok(HeartRateMeasurement { bpm, rr_intervals })
}

/// Parse battery level characteristic (0x2A19)
/// Single byte: 0-100 (percentage)
pub fn parse_battery(data: &[u8]) -> anyhow::Result<u8> {
    if data.is_empty() {
        anyhow::bail!("empty battery data");
    }
    let level = data[0];
    if level > 100 {
        anyhow::bail!("invalid battery level: {}", level);
    }
    Ok(level)
}

/// Parse step count characteristic (0x2A2C)
/// uint32 little-endian
pub fn parse_steps(data: &[u8]) -> anyhow::Result<u32> {
    if data.len() < 4 {
        anyhow::bail!("insufficient data for step count");
    }
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Device information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub model_number: Option<String>,
    pub firmware_revision: Option<String>,
    pub manufacturer_name: Option<String>,
}

/// Parse device information string characteristic
pub fn parse_device_info_string(data: &[u8]) -> anyhow::Result<String> {
    String::from_utf8(data.to_vec())
        .map_err(|e| anyhow::anyhow!("invalid UTF-8 in device info: {}", e))
}

// ---------------------------------------------------------------------------
// Device Configuration
// ---------------------------------------------------------------------------

/// BLE wearable device configuration
#[derive(Debug, Clone)]
pub struct BleDeviceConfig {
    pub mac_addr: String,
    pub name: String,
}

impl BleDeviceConfig {
    pub fn new(mac_addr: &str, name: &str) -> Self {
        Self {
            mac_addr: mac_addr.to_string(),
            name: name.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Wearable Data
// ---------------------------------------------------------------------------

/// Wearable device data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WearableData {
    pub heart_rate: Option<u16>,
    pub steps: Option<u32>,
    pub battery_pct: Option<u8>,
    pub device_info: DeviceInfo,
    pub connected: bool,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// BLE wearable adapter using a pluggable BLE transport
pub struct BleWearableAdapter {
    transport: Arc<dyn BleTransport>,
    devices: Arc<RwLock<HashMap<String, BleDeviceConfig>>>,
}

impl BleWearableAdapter {
    /// Create adapter with a custom transport (for testing)
    pub fn with_transport(transport: Arc<dyn BleTransport>) -> Self {
        Self {
            transport,
            devices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create adapter with mock transport for testing
    pub fn with_mock() -> (Self, Arc<MockBleTransport>) {
        let mock = Arc::new(MockBleTransport::new());
        let adapter = Self::with_transport(mock.clone());
        (adapter, mock)
    }

    /// Register a BLE wearable device
    pub async fn register_device(&self, device_id: &str, mac_addr: &str, name: &str) {
        let config = BleDeviceConfig::new(mac_addr, name);
        self.devices.write().await.insert(device_id.to_string(), config);
    }

    /// Get device configuration
    async fn get_device_config(&self, device_id: &str) -> anyhow::Result<BleDeviceConfig> {
        self.devices
            .read()
            .await
            .get(device_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown wearable device: {}", device_id))
    }

    /// Read heart rate from device
    pub async fn read_heart_rate(&self, device_id: &str) -> anyhow::Result<HeartRateMeasurement> {
        let _config = self.get_device_config(device_id).await?;
        let data = self
            .transport
            .read_characteristic(service_uuid::HEART_RATE, char_uuid::HEART_RATE_MEASUREMENT)
            .await?;
        parse_heart_rate(&data)
    }

    /// Read battery level from device
    pub async fn read_battery(&self, device_id: &str) -> anyhow::Result<u8> {
        let _config = self.get_device_config(device_id).await?;
        let data = self
            .transport
            .read_characteristic(service_uuid::BATTERY, char_uuid::BATTERY_LEVEL)
            .await?;
        parse_battery(&data)
    }

    /// Read step count from device
    pub async fn read_steps(&self, device_id: &str) -> anyhow::Result<u32> {
        let _config = self.get_device_config(device_id).await?;
        let data = self
            .transport
            .read_characteristic(service_uuid::USER_DATA, char_uuid::STEP_COUNT)
            .await?;
        parse_steps(&data)
    }

    /// Read device information
    pub async fn read_device_info(&self, device_id: &str) -> anyhow::Result<DeviceInfo> {
        let _config = self.get_device_config(device_id).await?;

        let model_data = self
            .transport
            .read_characteristic(service_uuid::DEVICE_INFO, char_uuid::MODEL_NUMBER)
            .await
            .ok();
        let firmware_data = self
            .transport
            .read_characteristic(service_uuid::DEVICE_INFO, char_uuid::FIRMWARE_REVISION)
            .await
            .ok();
        let manufacturer_data = self
            .transport
            .read_characteristic(service_uuid::DEVICE_INFO, char_uuid::MANUFACTURER_NAME)
            .await
            .ok();

        Ok(DeviceInfo {
            model_number: model_data.and_then(|d| parse_device_info_string(&d).ok()),
            firmware_revision: firmware_data.and_then(|d| parse_device_info_string(&d).ok()),
            manufacturer_name: manufacturer_data.and_then(|d| parse_device_info_string(&d).ok()),
        })
    }

    /// Sync time to device
    pub async fn sync_time(&self, device_id: &str) -> anyhow::Result<()> {
        let _config = self.get_device_config(device_id).await?;

        // Current time in GATT format (simplified: just timestamp as bytes)
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let time_bytes = timestamp.to_le_bytes();

        self.transport
            .write_characteristic(
                service_uuid::CURRENT_TIME,
                char_uuid::CURRENT_TIME,
                &time_bytes,
            )
            .await
    }
}

#[async_trait]
impl DeviceController for BleWearableAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let config = self.get_device_config(device_id).await?;
        self.transport.connect(&config.mac_addr).await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let _config = self.get_device_config(device_id).await?;
        self.transport.disconnect().await
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        _value: Value,
    ) -> anyhow::Result<()> {
        match property {
            "sync" => self.sync_time(device_id).await,
            _ => anyhow::bail!("unsupported wearable property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let config = self.get_device_config(device_id).await?;

        let heart_rate = self.read_heart_rate(device_id).await.ok().map(|hr| hr.bpm);
        let battery_pct = self.read_battery(device_id).await.ok();
        let steps = self.read_steps(device_id).await.ok();
        let device_info = self.read_device_info(device_id).await.unwrap_or_default();

        let data = WearableData {
            heart_rate,
            steps,
            battery_pct,
            device_info,
            connected: true,
        };

        let mut state = serde_json::to_value(data)?;
        state["device_name"] = Value::String(config.name);
        state["mac_addr"] = Value::String(config.mac_addr);

        Ok(state)
    }

    fn supported_type(&self) -> &str {
        "Wearable"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to setup adapter with mock transport
    async fn setup() -> (BleWearableAdapter, Arc<MockBleTransport>) {
        let (adapter, mock) = BleWearableAdapter::with_mock();
        adapter.register_device("band-001", "AA:BB:CC:DD:EE:FF", "Test Band").await;

        // Setup mock responses
        mock.set_response(
            service_uuid::HEART_RATE,
            char_uuid::HEART_RATE_MEASUREMENT,
            vec![0x00, 72], // uint8 HR, no RR intervals
        ).await;
        mock.set_response(
            service_uuid::BATTERY,
            char_uuid::BATTERY_LEVEL,
            vec![85],
        ).await;
        mock.set_response(
            service_uuid::USER_DATA,
            char_uuid::STEP_COUNT,
            vec![0x10, 0x27, 0x00, 0x00], // 10000 steps
        ).await;
        mock.set_response(
            service_uuid::DEVICE_INFO,
            char_uuid::MODEL_NUMBER,
            b"Band-X1".to_vec(),
        ).await;
        mock.set_response(
            service_uuid::DEVICE_INFO,
            char_uuid::FIRMWARE_REVISION,
            b"1.2.3".to_vec(),
        ).await;
        mock.set_response(
            service_uuid::DEVICE_INFO,
            char_uuid::MANUFACTURER_NAME,
            b"TestCorp".to_vec(),
        ).await;

        (adapter, mock)
    }

    // 1. Parse heart rate uint8 format
    #[test]
    fn parse_heart_rate_uint8() {
        let data = vec![0x00, 72]; // flags=0, HR=72
        let hr = parse_heart_rate(&data).unwrap();
        assert_eq!(hr.bpm, 72);
        assert_eq!(hr.rr_intervals.len(), 0);
    }

    // 2. Parse heart rate uint16 format
    #[test]
    fn parse_heart_rate_uint16() {
        let data = vec![0x01, 0x50, 0x00]; // flags=1, HR=80 (little-endian)
        let hr = parse_heart_rate(&data).unwrap();
        assert_eq!(hr.bpm, 80);
        assert_eq!(hr.rr_intervals.len(), 0);
    }

    // 3. Parse heart rate with RR intervals
    #[test]
    fn parse_heart_rate_with_rr_intervals() {
        let data = vec![0x10, 75, 0xE8, 0x03, 0xF4, 0x01]; // flags=0x10, HR=75, RR=[1000, 500]
        let hr = parse_heart_rate(&data).unwrap();
        assert_eq!(hr.bpm, 75);
        assert_eq!(hr.rr_intervals, vec![1000, 500]);
    }

    // 4. Parse heart rate uint16 with RR intervals
    #[test]
    fn parse_heart_rate_uint16_with_rr() {
        let data = vec![0x11, 0x5A, 0x00, 0xD0, 0x07]; // flags=0x11, HR=90, RR=[2000]
        let hr = parse_heart_rate(&data).unwrap();
        assert_eq!(hr.bpm, 90);
        assert_eq!(hr.rr_intervals, vec![2000]);
    }

    // 5. Parse heart rate empty data error
    #[test]
    fn parse_heart_rate_empty_error() {
        assert!(parse_heart_rate(&[]).is_err());
    }

    // 6. Parse battery level
    #[test]
    fn parse_battery_valid() {
        assert_eq!(parse_battery(&[85]).unwrap(), 85);
        assert_eq!(parse_battery(&[0]).unwrap(), 0);
        assert_eq!(parse_battery(&[100]).unwrap(), 100);
    }

    // 7. Parse battery level invalid
    #[test]
    fn parse_battery_invalid() {
        assert!(parse_battery(&[]).is_err());
        assert!(parse_battery(&[101]).is_err());
    }

    // 8. Parse step count
    #[test]
    fn parse_steps_valid() {
        let data = vec![0x10, 0x27, 0x00, 0x00]; // 10000 little-endian
        assert_eq!(parse_steps(&data).unwrap(), 10000);
    }

    // 9. Parse step count insufficient data
    #[test]
    fn parse_steps_insufficient_data() {
        assert!(parse_steps(&[0x10, 0x27]).is_err());
    }

    // 10. Parse device info string
    #[test]
    fn parse_device_info_string_valid() {
        let data = b"Band-X1";
        assert_eq!(parse_device_info_string(data).unwrap(), "Band-X1");
    }

    // 11. Mock transport connect/disconnect
    #[tokio::test]
    async fn mock_transport_connect_disconnect() {
        let mock = MockBleTransport::new();
        assert!(!mock.is_connected().await);

        mock.connect("AA:BB:CC:DD:EE:FF").await.unwrap();
        assert!(mock.is_connected().await);

        mock.disconnect().await.unwrap();
        assert!(!mock.is_connected().await);

        let ops = mock.operations().await;
        assert_eq!(ops.len(), 2);
    }

    // 12. Mock transport read characteristic
    #[tokio::test]
    async fn mock_transport_read_characteristic() {
        let mock = MockBleTransport::new();
        mock.set_response(service_uuid::BATTERY, char_uuid::BATTERY_LEVEL, vec![75]).await;

        let data = mock
            .read_characteristic(service_uuid::BATTERY, char_uuid::BATTERY_LEVEL)
            .await
            .unwrap();
        assert_eq!(data, vec![75]);
    }

    // 13. DeviceController turn_on connects
    #[tokio::test]
    async fn device_controller_turn_on_connects() {
        let (adapter, mock) = setup().await;
        adapter.turn_on("band-001").await.unwrap();
        assert!(mock.is_connected().await);
    }

    // 14. DeviceController turn_off disconnects
    #[tokio::test]
    async fn device_controller_turn_off_disconnects() {
        let (adapter, mock) = setup().await;
        adapter.turn_on("band-001").await.unwrap();
        adapter.turn_off("band-001").await.unwrap();
        assert!(!mock.is_connected().await);
    }

    // 15. DeviceController set_property sync
    #[tokio::test]
    async fn device_controller_set_property_sync() {
        let (adapter, _mock) = setup().await;
        let result = adapter.set_property("band-001", "sync", Value::Null).await;
        assert!(result.is_ok());
    }

    // 16. DeviceController set_property unsupported
    #[tokio::test]
    async fn device_controller_set_property_unsupported() {
        let (adapter, _mock) = setup().await;
        let result = adapter.set_property("band-001", "reboot", Value::Null).await;
        assert!(result.is_err());
    }

    // 17. DeviceController get_state returns full data
    #[tokio::test]
    async fn device_controller_get_state_full_data() {
        let (adapter, _mock) = setup().await;
        let state = adapter.get_state("band-001").await.unwrap();

        assert_eq!(state["heart_rate"], 72);
        assert_eq!(state["battery_pct"], 85);
        assert_eq!(state["steps"], 10000);
        assert_eq!(state["device_info"]["model_number"], "Band-X1");
        assert_eq!(state["device_info"]["firmware_revision"], "1.2.3");
        assert_eq!(state["device_info"]["manufacturer_name"], "TestCorp");
        assert_eq!(state["device_name"], "Test Band");
        assert_eq!(state["mac_addr"], "AA:BB:CC:DD:EE:FF");
    }

    // 18. Unknown device errors
    #[tokio::test]
    async fn unknown_device_errors() {
        let (adapter, _mock) = BleWearableAdapter::with_mock();
        assert!(adapter.turn_on("unknown").await.is_err());
        assert!(adapter.get_state("unknown").await.is_err());
    }

    // 19. Read heart rate
    #[tokio::test]
    async fn read_heart_rate_works() {
        let (adapter, _mock) = setup().await;
        let hr = adapter.read_heart_rate("band-001").await.unwrap();
        assert_eq!(hr.bpm, 72);
    }

    // 20. Read battery
    #[tokio::test]
    async fn read_battery_works() {
        let (adapter, _mock) = setup().await;
        let battery = adapter.read_battery("band-001").await.unwrap();
        assert_eq!(battery, 85);
    }

    // 21. Read steps
    #[tokio::test]
    async fn read_steps_works() {
        let (adapter, _mock) = setup().await;
        let steps = adapter.read_steps("band-001").await.unwrap();
        assert_eq!(steps, 10000);
    }

    // 22. Read device info
    #[tokio::test]
    async fn read_device_info_works() {
        let (adapter, _mock) = setup().await;
        let info = adapter.read_device_info("band-001").await.unwrap();
        assert_eq!(info.model_number, Some("Band-X1".to_string()));
        assert_eq!(info.firmware_revision, Some("1.2.3".to_string()));
        assert_eq!(info.manufacturer_name, Some("TestCorp".to_string()));
    }

    // 23. Supported type is Wearable
    #[tokio::test]
    async fn supported_type_is_wearable() {
        let (adapter, _mock) = BleWearableAdapter::with_mock();
        assert_eq!(adapter.supported_type(), "Wearable");
    }

    // 24. Sync time writes to characteristic
    #[tokio::test]
    async fn sync_time_writes_characteristic() {
        let (adapter, mock) = setup().await;
        adapter.sync_time("band-001").await.unwrap();

        let ops = mock.operations().await;
        let write_op = ops.iter().find(|op| matches!(op, BleOperation::WriteCharacteristic { .. }));
        assert!(write_op.is_some());
    }
}
