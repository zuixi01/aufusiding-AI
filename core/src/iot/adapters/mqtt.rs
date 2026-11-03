//! MQTT adapter
//!
//! Controls Zigbee2MQTT/Tasmota/ESPHome devices via an MQTT broker.
//! Uses a `MqttTransport` trait abstraction for testability.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// QoS
// ---------------------------------------------------------------------------

/// MQTT Quality of Service level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

#[allow(clippy::derivable_impls)]
impl Default for QoS {
    fn default() -> Self {
        QoS::AtLeastOnce
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// MQTT connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub broker_host: String,
    pub broker_port: u16,
    pub client_id: String,
    pub topic_prefix: String,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub qos: QoS,
    /// Last Will and Testament topic (optional).
    pub lwt_topic: Option<String>,
    /// Last Will and Testament payload (optional).
    pub lwt_payload: Option<String>,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker_host: "localhost".into(),
            broker_port: 1883,
            client_id: "prometheus-iot".into(),
            topic_prefix: "zigbee2mqtt".into(),
            username: None,
            password: None,
            qos: QoS::default(),
            lwt_topic: None,
            lwt_payload: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Transport trait
// ---------------------------------------------------------------------------

/// Abstraction over MQTT publish/subscribe for testability.
#[async_trait]
pub trait MqttTransport: Send + Sync {
    /// Publish a message to a topic.
    async fn publish(
        &self,
        topic: &str,
        payload: &[u8],
        qos: QoS,
        retain: bool,
    ) -> anyhow::Result<()>;

    /// Subscribe to a topic and return the latest message payload (if any).
    async fn subscribe(&self, topic: &str, qos: QoS) -> anyhow::Result<Option<Vec<u8>>>;
}

// ---------------------------------------------------------------------------
// Mock transport (for testing)
// ---------------------------------------------------------------------------

/// Recorded publish call.
#[derive(Debug, Clone)]
pub struct MqttCommand {
    pub topic: String,
    pub payload: Value,
    pub qos: QoS,
    pub retain: bool,
}

/// Mock MQTT transport that records publishes and returns injected state.
pub struct MockMqttTransport {
    pub commands: Arc<RwLock<Vec<MqttCommand>>>,
    pub subscriptions: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl MockMqttTransport {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(Vec::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Inject a message that will be returned on subscribe.
    pub async fn inject_message(&self, topic: &str, payload: &[u8]) {
        self.subscriptions
            .write()
            .await
            .insert(topic.to_string(), payload.to_vec());
    }

    /// Get recorded commands.
    pub async fn commands(&self) -> Vec<MqttCommand> {
        self.commands.read().await.clone()
    }
}

#[async_trait]
impl MqttTransport for MockMqttTransport {
    async fn publish(
        &self,
        topic: &str,
        payload: &[u8],
        qos: QoS,
        retain: bool,
    ) -> anyhow::Result<()> {
        let value: Value = serde_json::from_slice(payload)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(payload).to_string()));
        self.commands.write().await.push(MqttCommand {
            topic: topic.to_string(),
            payload: value,
            qos,
            retain,
        });
        Ok(())
    }

    async fn subscribe(&self, topic: &str, _qos: QoS) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.subscriptions.read().await.get(topic).cloned())
    }
}

// ---------------------------------------------------------------------------
// Tokio TCP transport (real MQTT v3.1.1)
// ---------------------------------------------------------------------------

/// MQTT v3.1.1 packet encoder/decoder helpers.
mod mqtt_codec {
    /// Encode a remaining-length field per MQTT spec.
    pub fn encode_remaining_length(mut len: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            let mut byte = (len % 128) as u8;
            len /= 128;
            if len > 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if len == 0 {
                break;
            }
        }
        bytes
    }

    /// Build a CONNECT packet (MQTT v3.1.1).
    pub fn build_connect_packet(
        client_id: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Vec<u8> {
        let mut var_header = Vec::new();
        // Protocol Name
        var_header.extend_from_slice(&[0x00, 0x04]); // length
        var_header.extend_from_slice(b"MQTT");
        // Protocol Level (4 = v3.1.1)
        var_header.push(0x04);
        // Connect Flags
        let mut flags: u8 = 0x02; // Clean Session
        if username.is_some() {
            flags |= 0x80;
        }
        if password.is_some() {
            flags |= 0x40;
        }
        var_header.push(flags);
        // Keep Alive (60 seconds)
        var_header.extend_from_slice(&[0x00, 0x3C]);

        let mut payload = Vec::new();
        // Client ID
        let id_bytes = client_id.as_bytes();
        payload.extend_from_slice(&(id_bytes.len() as u16).to_be_bytes());
        payload.extend_from_slice(id_bytes);
        // Username
        if let Some(u) = username {
            let u_bytes = u.as_bytes();
            payload.extend_from_slice(&(u_bytes.len() as u16).to_be_bytes());
            payload.extend_from_slice(u_bytes);
        }
        // Password
        if let Some(p) = password {
            let p_bytes = p.as_bytes();
            payload.extend_from_slice(&(p_bytes.len() as u16).to_be_bytes());
            payload.extend_from_slice(p_bytes);
        }

        let remaining = var_header.len() + payload.len();
        let mut packet = vec![0x10]; // CONNECT packet type
        packet.extend(encode_remaining_length(remaining));
        packet.extend(var_header);
        packet.extend(payload);
        packet
    }

    /// Build a PUBLISH packet (QoS 0 only for simplicity).
    pub fn build_publish_packet(topic: &str, payload: &[u8], retain: bool) -> Vec<u8> {
        let mut first_byte: u8 = 0x30; // PUBLISH
        if retain {
            first_byte |= 0x01;
        }

        let mut var_header = Vec::new();
        let topic_bytes = topic.as_bytes();
        var_header.extend_from_slice(&(topic_bytes.len() as u16).to_be_bytes());
        var_header.extend_from_slice(topic_bytes);

        let remaining = var_header.len() + payload.len();
        let mut packet = vec![first_byte];
        packet.extend(encode_remaining_length(remaining));
        packet.extend(var_header);
        packet.extend_from_slice(payload);
        packet
    }

    /// Build a SUBSCRIBE packet.
    pub fn build_subscribe_packet(topic: &str, qos: u8, packet_id: u16) -> Vec<u8> {
        let mut var_header = Vec::new();
        var_header.extend_from_slice(&packet_id.to_be_bytes());

        let mut payload = Vec::new();
        let topic_bytes = topic.as_bytes();
        payload.extend_from_slice(&(topic_bytes.len() as u16).to_be_bytes());
        payload.extend_from_slice(topic_bytes);
        payload.push(qos);

        let remaining = var_header.len() + payload.len();
        let mut packet = vec![0x82]; // SUBSCRIBE
        packet.extend(encode_remaining_length(remaining));
        packet.extend(var_header);
        packet.extend(payload);
        packet
    }
}

/// Real MQTT transport using `tokio::net::TcpStream`.
///
/// Implements basic MQTT v3.1.1 CONNECT, PUBLISH, and SUBSCRIBE.
pub struct TokioMqttTransport {
    config: MqttConfig,
    stream: Arc<RwLock<Option<tokio::net::TcpStream>>>,
    next_packet_id: Arc<RwLock<u16>>,
}

impl TokioMqttTransport {
    pub fn new(config: MqttConfig) -> Self {
        Self {
            config,
            stream: Arc::new(RwLock::new(None)),
            next_packet_id: Arc::new(RwLock::new(1)),
        }
    }

    /// Connect to the MQTT broker and send a CONNECT packet.
    pub async fn connect(&self) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;

        let addr = format!("{}:{}", self.config.broker_host, self.config.broker_port);
        let tcp = tokio::net::TcpStream::connect(&addr).await?;

        let connect_packet = mqtt_codec::build_connect_packet(
            &self.config.client_id,
            self.config.username.as_deref(),
            self.config.password.as_deref(),
        );

        let mut guard = self.stream.write().await;
        *guard = Some(tcp);
        if let Some(ref mut stream) = *guard {
            stream.write_all(&connect_packet).await?;
        }

        Ok(())
    }

    async fn next_packet_id(&self) -> u16 {
        let mut id = self.next_packet_id.write().await;
        let current = *id;
        *id = id.wrapping_add(1);
        if *id == 0 {
            *id = 1;
        }
        current
    }
}

#[async_trait]
impl MqttTransport for TokioMqttTransport {
    async fn publish(
        &self,
        topic: &str,
        payload: &[u8],
        _qos: QoS,
        retain: bool,
    ) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;

        let packet = mqtt_codec::build_publish_packet(topic, payload, retain);
        let mut guard = self.stream.write().await;
        match guard.as_mut() {
            Some(stream) => {
                stream.write_all(&packet).await?;
                Ok(())
            }
            None => anyhow::bail!("MQTT: not connected"),
        }
    }

    async fn subscribe(&self, topic: &str, qos: QoS) -> anyhow::Result<Option<Vec<u8>>> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let packet_id = self.next_packet_id().await;
        let packet = mqtt_codec::build_subscribe_packet(topic, qos as u8, packet_id);

        let mut guard = self.stream.write().await;
        let stream = match guard.as_mut() {
            Some(s) => s,
            None => anyhow::bail!("MQTT: not connected"),
        };

        stream.write_all(&packet).await?;

        // Try to read a response with a short timeout
        let mut buf = [0u8; 4096];
        match tokio::time::timeout(tokio::time::Duration::from_secs(2), stream.read(&mut buf)).await
        {
            Ok(Ok(n)) if n > 0 => Ok(Some(buf[..n].to_vec())),
            _ => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// MQTT adapter using a pluggable transport.
pub struct MqttAdapter {
    config: MqttConfig,
    transport: Arc<dyn MqttTransport>,
    states: Arc<RwLock<HashMap<String, Value>>>,
}

impl MqttAdapter {
    pub fn new(config: MqttConfig, transport: Arc<dyn MqttTransport>) -> Self {
        Self {
            config,
            transport,
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Convenience constructor with mock transport (for backward compat / simple tests).
    pub fn with_mock(config: MqttConfig) -> (Self, Arc<MockMqttTransport>) {
        let mock = Arc::new(MockMqttTransport::new());
        let adapter = Self::new(config, mock.clone());
        (adapter, mock)
    }

    /// Build the command topic for a device.
    fn command_topic(&self, device_id: &str) -> String {
        format!("{}/{}/set", self.config.topic_prefix, device_id)
    }

    /// Build the state topic for a device.
    fn state_topic(&self, device_id: &str) -> String {
        format!("{}/{}", self.config.topic_prefix, device_id)
    }

    /// Publish a JSON command to a device.
    async fn publish_command(
        &self,
        device_id: &str,
        payload: Value,
        retain: bool,
    ) -> anyhow::Result<()> {
        let topic = self.command_topic(device_id);
        let bytes = serde_json::to_vec(&payload)?;
        self.transport
            .publish(&topic, &bytes, self.config.qos, retain)
            .await
    }

    /// Subscribe to a device's state topic and parse JSON.
    pub async fn subscribe_state(&self, device_id: &str) -> anyhow::Result<Option<Value>> {
        let topic = self.state_topic(device_id);
        match self.transport.subscribe(&topic, self.config.qos).await? {
            Some(bytes) => {
                let value: Value = serde_json::from_slice(&bytes)?;
                // Cache the state
                self.states
                    .write()
                    .await
                    .insert(device_id.to_string(), value.clone());
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Inject device state (for testing or from MQTT message callback).
    pub async fn inject_state(&self, device_id: &str, state: Value) {
        self.states
            .write()
            .await
            .insert(device_id.to_string(), state);
    }

    pub fn config(&self) -> &MqttConfig {
        &self.config
    }
}

#[async_trait]
impl DeviceController for MqttAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.publish_command(device_id, serde_json::json!({"state": "ON"}), false)
            .await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        self.publish_command(device_id, serde_json::json!({"state": "OFF"}), false)
            .await
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        self.publish_command(device_id, serde_json::json!({ property: value }), false)
            .await
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        // Try subscribing first to get fresh state
        if let Some(state) = self.subscribe_state(device_id).await? {
            return Ok(state);
        }
        // Fall back to cached state
        self.states
            .read()
            .await
            .get(device_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no cached state for MQTT device: {}", device_id))
    }

    fn supported_type(&self) -> &str {
        "mqtt"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (MqttAdapter, Arc<MockMqttTransport>) {
        MqttAdapter::with_mock(MqttConfig::default())
    }

    // 1. supported_type
    #[test]
    fn mqtt_supported_type() {
        let (adapter, _) = setup();
        assert_eq!(adapter.supported_type(), "mqtt");
    }

    // 2. default config values
    #[test]
    fn mqtt_default_config() {
        let config = MqttConfig::default();
        assert_eq!(config.broker_host, "localhost");
        assert_eq!(config.broker_port, 1883);
        assert_eq!(config.topic_prefix, "zigbee2mqtt");
        assert_eq!(config.qos, QoS::AtLeastOnce);
        assert!(config.lwt_topic.is_none());
    }

    // 3. turn_on publishes ON command
    #[tokio::test]
    async fn mqtt_turn_on_publishes_command() {
        let (adapter, mock) = setup();
        adapter.turn_on("bulb-001").await.unwrap();
        let cmds = mock.commands().await;
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].topic, "zigbee2mqtt/bulb-001/set");
        assert_eq!(cmds[0].payload["state"], "ON");
        assert_eq!(cmds[0].qos, QoS::AtLeastOnce);
        assert!(!cmds[0].retain);
    }

    // 4. turn_off publishes OFF command
    #[tokio::test]
    async fn mqtt_turn_off_publishes_command() {
        let (adapter, mock) = setup();
        adapter.turn_off("bulb-001").await.unwrap();
        let cmds = mock.commands().await;
        assert_eq!(cmds[0].payload["state"], "OFF");
    }

    // 5. set_property publishes property command
    #[tokio::test]
    async fn mqtt_set_property() {
        let (adapter, mock) = setup();
        adapter
            .set_property("bulb-001", "brightness", serde_json::json!(80))
            .await
            .unwrap();
        let cmds = mock.commands().await;
        assert_eq!(cmds[0].payload["brightness"], 80);
    }

    // 6. get_state from cache
    #[tokio::test]
    async fn mqtt_get_state_cached() {
        let (adapter, _) = setup();
        adapter
            .inject_state(
                "bulb-001",
                serde_json::json!({"state": "ON", "brightness": 100}),
            )
            .await;
        let state = adapter.get_state("bulb-001").await.unwrap();
        assert_eq!(state["state"], "ON");
    }

    // 7. get_state unknown device errors
    #[tokio::test]
    async fn mqtt_get_state_unknown_errors() {
        let (adapter, _) = setup();
        assert!(adapter.get_state("unknown").await.is_err());
    }

    // 8. subscribe_state parses JSON from transport
    #[tokio::test]
    async fn mqtt_subscribe_state() {
        let (adapter, mock) = setup();
        let payload = serde_json::json!({"state": "ON", "brightness": 50});
        mock.inject_message(
            "zigbee2mqtt/bulb-001",
            serde_json::to_vec(&payload).unwrap().as_slice(),
        )
        .await;
        let state = adapter.subscribe_state("bulb-001").await.unwrap().unwrap();
        assert_eq!(state["brightness"], 50);
    }

    // 9. subscribe_state returns None when no message
    #[tokio::test]
    async fn mqtt_subscribe_state_none() {
        let (adapter, _) = setup();
        let result = adapter.subscribe_state("unknown").await.unwrap();
        assert!(result.is_none());
    }

    // 10. get_state prefers subscribed state over cache
    #[tokio::test]
    async fn mqtt_get_state_prefers_subscribe() {
        let (adapter, mock) = setup();
        adapter
            .inject_state("bulb-001", serde_json::json!({"brightness": 10}))
            .await;
        let fresh = serde_json::json!({"brightness": 99});
        mock.inject_message(
            "zigbee2mqtt/bulb-001",
            serde_json::to_vec(&fresh).unwrap().as_slice(),
        )
        .await;
        let state = adapter.get_state("bulb-001").await.unwrap();
        assert_eq!(state["brightness"], 99);
    }

    // 11. command topic format
    #[test]
    fn mqtt_command_topic_format() {
        let (adapter, _) = setup();
        assert_eq!(
            adapter.command_topic("bulb-001"),
            "zigbee2mqtt/bulb-001/set"
        );
    }

    // 12. state topic format
    #[test]
    fn mqtt_state_topic_format() {
        let (adapter, _) = setup();
        assert_eq!(adapter.state_topic("bulb-001"), "zigbee2mqtt/bulb-001");
    }

    // 13. custom topic prefix
    #[tokio::test]
    async fn mqtt_custom_prefix() {
        let config = MqttConfig {
            topic_prefix: "tasmota".into(),
            ..MqttConfig::default()
        };
        let (adapter, mock) = config.clone().into_adapter();
        adapter.turn_on("switch-001").await.unwrap();
        let cmds = mock.commands().await;
        assert_eq!(cmds[0].topic, "tasmota/switch-001/set");
    }

    // 14. QoS levels
    #[tokio::test]
    async fn mqtt_qos_levels() {
        let config = MqttConfig {
            qos: QoS::ExactlyOnce,
            ..MqttConfig::default()
        };
        let (adapter, mock) = config.clone().into_adapter();
        adapter.turn_on("bulb-001").await.unwrap();
        let cmds = mock.commands().await;
        assert_eq!(cmds[0].qos, QoS::ExactlyOnce);
    }

    // 15. multiple commands accumulate
    #[tokio::test]
    async fn mqtt_multiple_commands() {
        let (adapter, mock) = setup();
        adapter.turn_on("bulb-001").await.unwrap();
        adapter
            .set_property("bulb-001", "brightness", serde_json::json!(50))
            .await
            .unwrap();
        adapter.turn_off("bulb-001").await.unwrap();
        let cmds = mock.commands().await;
        assert_eq!(cmds.len(), 3);
    }

    // 16. LWT config is stored
    #[test]
    fn mqtt_lwt_config() {
        let config = MqttConfig {
            lwt_topic: Some("prometheus/status".into()),
            lwt_payload: Some("offline".into()),
            ..MqttConfig::default()
        };
        assert_eq!(config.lwt_topic.as_deref(), Some("prometheus/status"));
        assert_eq!(config.lwt_payload.as_deref(), Some("offline"));
    }

    // Helper trait for ergonomic test setup with custom config
    trait IntoAdapter {
        fn into_adapter(self) -> (MqttAdapter, Arc<MockMqttTransport>);
    }

    impl IntoAdapter for MqttConfig {
        fn into_adapter(self) -> (MqttAdapter, Arc<MockMqttTransport>) {
            MqttAdapter::with_mock(self)
        }
    }

    // 17. MQTT codec: remaining length encoding
    #[test]
    fn mqtt_codec_remaining_length() {
        assert_eq!(super::mqtt_codec::encode_remaining_length(0), vec![0x00]);
        assert_eq!(super::mqtt_codec::encode_remaining_length(127), vec![0x7F]);
        assert_eq!(
            super::mqtt_codec::encode_remaining_length(128),
            vec![0x80, 0x01]
        );
    }

    // 18. MQTT codec: CONNECT packet structure
    #[test]
    fn mqtt_codec_connect_packet() {
        let packet = super::mqtt_codec::build_connect_packet("test-client", None, None);
        assert_eq!(packet[0], 0x10); // CONNECT type
                                     // Protocol name "MQTT"
        let remaining_start = 2; // 1 byte type + 1 byte remaining length (small)
        assert_eq!(&packet[remaining_start + 2..remaining_start + 6], b"MQTT");
        assert_eq!(packet[remaining_start + 6], 0x04); // Protocol level v3.1.1
    }

    // 19. MQTT codec: PUBLISH packet structure
    #[test]
    fn mqtt_codec_publish_packet() {
        let packet = super::mqtt_codec::build_publish_packet("test/topic", b"hello", false);
        assert_eq!(packet[0] & 0xF0, 0x30); // PUBLISH type
        assert_eq!(packet[0] & 0x01, 0x00); // no retain
    }

    // 20. MQTT codec: PUBLISH with retain
    #[test]
    fn mqtt_codec_publish_retain() {
        let packet = super::mqtt_codec::build_publish_packet("test/topic", b"hello", true);
        assert_eq!(packet[0] & 0x01, 0x01); // retain flag set
    }

    // 21. MQTT codec: SUBSCRIBE packet structure
    #[test]
    fn mqtt_codec_subscribe_packet() {
        let packet = super::mqtt_codec::build_subscribe_packet("test/topic", 1, 42);
        assert_eq!(packet[0], 0x82); // SUBSCRIBE type
    }

    // 22. MQTT codec: CONNECT with credentials
    #[test]
    fn mqtt_codec_connect_with_credentials() {
        let packet = super::mqtt_codec::build_connect_packet("client", Some("user"), Some("pass"));
        let remaining_start = 2;
        let flags = packet[remaining_start + 7];
        assert!(flags & 0x80 != 0); // username flag
        assert!(flags & 0x40 != 0); // password flag
    }

    // 23. TokioMqttTransport can be constructed
    #[test]
    fn tokio_mqtt_transport_new() {
        let config = MqttConfig::default();
        let _transport = TokioMqttTransport::new(config);
    }
}
