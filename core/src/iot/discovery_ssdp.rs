//! SSDP/UPnP 发现后端
//!
//! 通过 SSDP 多播发现 UPnP 设备：DLNA 音响/电视、NAS、路由器、摄像头等。

use async_trait::async_trait;
use std::collections::HashMap;

use super::discovery::DeviceInfo;
use super::discovery_provider::DiscoveryProvider;

/// UPnP 设备类型 URN 到设备类型的映射
const URN_MAPPINGS: &[(&str, &str)] = &[
    ("MediaRenderer", "Speaker"),
    ("MediaServer", "Nas"),
    ("InternetGatewayDevice", "Computer"),
    ("Basic", "Sensor"),
    ("DimmableLight", "Light"),
    ("BinaryLight", "Light"),
    ("HVAC", "Thermostat"),
    ("DigitalSecurityCamera", "Camera"),
];

/// SSDP 响应数据
#[derive(Debug, Clone)]
pub struct SsdpResponse {
    pub location: String,
    pub search_target: String,
    pub usn: String,
    pub server: String,
}

/// SSDP UDP socket 抽象
#[async_trait]
pub trait SsdpSocket: Send + Sync {
    async fn send_msearch(&self) -> anyhow::Result<()>;
    async fn recv_responses(&self, timeout_secs: u64) -> anyhow::Result<Vec<SsdpResponse>>;
}

/// 默认 SSDP socket（返回空结果）
pub struct DefaultSsdpSocket;

#[async_trait]
impl SsdpSocket for DefaultSsdpSocket {
    async fn send_msearch(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn recv_responses(&self, _timeout_secs: u64) -> anyhow::Result<Vec<SsdpResponse>> {
        Ok(Vec::new())
    }
}

/// Real SSDP socket using UDP multicast to 239.255.255.250:1900.
///
/// Falls back to empty results with a warning log if the network is unavailable.
pub struct RealSsdpSocket {
    socket: tokio::sync::RwLock<Option<tokio::net::UdpSocket>>,
}

impl RealSsdpSocket {
    pub fn new() -> Self {
        Self {
            socket: tokio::sync::RwLock::new(None),
        }
    }
}

const SSDP_MULTICAST_ADDR: &str = "239.255.255.250:1900";

#[async_trait]
impl SsdpSocket for RealSsdpSocket {
    async fn send_msearch(&self) -> anyhow::Result<()> {
        let sock = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("SSDP: failed to bind UDP socket: {}", e);
                return Ok(());
            }
        };

        let request = build_msearch_request();
        if let Err(e) = sock.send_to(&request, SSDP_MULTICAST_ADDR).await {
            tracing::warn!(
                "SSDP: failed to send M-SEARCH to {}: {}",
                SSDP_MULTICAST_ADDR,
                e
            );
        }

        *self.socket.write().await = Some(sock);
        Ok(())
    }

    async fn recv_responses(&self, timeout_secs: u64) -> anyhow::Result<Vec<SsdpResponse>> {
        let guard = self.socket.read().await;
        let sock = match guard.as_ref() {
            Some(s) => s,
            None => {
                tracing::warn!("SSDP: no socket available (send_msearch not called or failed)");
                return Ok(Vec::new());
            }
        };

        let mut responses = Vec::new();
        let mut buf = [0u8; 4096];
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, sock.recv_from(&mut buf)).await {
                Ok(Ok((len, _addr))) => {
                    let text = String::from_utf8_lossy(&buf[..len]);
                    if let Some(resp) = parse_ssdp_response(&text) {
                        responses.push(resp);
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("SSDP: recv error: {}", e);
                    break;
                }
                Err(_) => {
                    // Timeout reached
                    break;
                }
            }
        }

        Ok(responses)
    }
}

/// SSDP 发现后端
pub struct SsdpDiscoveryProvider {
    socket: Box<dyn SsdpSocket>,
}

impl SsdpDiscoveryProvider {
    pub fn new() -> Self {
        Self {
            socket: Box::new(DefaultSsdpSocket),
        }
    }

    pub fn with_socket(socket: Box<dyn SsdpSocket>) -> Self {
        Self { socket }
    }

    /// 根据 UPnP 设备类型 URN 推断设备类型
    fn infer_from_urn(urn: &str) -> &str {
        for (pattern, dtype) in URN_MAPPINGS {
            if urn.contains(pattern) {
                return dtype;
            }
        }
        "Sensor"
    }
}

#[async_trait]
impl DiscoveryProvider for SsdpDiscoveryProvider {
    fn name(&self) -> &str {
        "SSDP"
    }

    async fn scan(&self, timeout_secs: u64) -> anyhow::Result<Vec<DeviceInfo>> {
        self.socket.send_msearch().await?;
        let responses = self.socket.recv_responses(timeout_secs).await?;

        // 转换为 DeviceInfo 并按 USN 去重
        let mut seen = HashMap::new();
        let mut devices = Vec::new();

        for response in responses {
            if seen.contains_key(&response.usn) {
                continue;
            }
            seen.insert(response.usn.clone(), true);

            // 从 location 提取 IP 和端口
            let (ip, port) = parse_location(&response.location);
            let device_type = Self::infer_from_urn(&response.search_target);

            devices.push(DeviceInfo {
                id: format!("ssdp-{}", uuid::Uuid::new_v4()),
                name: response.server.clone(),
                device_type: device_type.to_string(),
                ip,
                port,
                protocol: "upnp".to_string(),
            });
        }

        Ok(devices)
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// 构建 SSDP M-SEARCH 请求
pub fn build_msearch_request() -> Vec<u8> {
    let request = "M-SEARCH * HTTP/1.1\r\n\
                   HOST: 239.255.255.250:1900\r\n\
                   MAN: \"ssdp:discover\"\r\n\
                   MX: 3\r\n\
                   ST: ssdp:all\r\n\
                   \r\n";
    request.as_bytes().to_vec()
}

/// 解析 SSDP 响应
pub fn parse_ssdp_response(data: &str) -> Option<SsdpResponse> {
    let mut location = None;
    let mut search_target = None;
    let mut usn = None;
    let mut server = None;

    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim().to_lowercase();
            let value = line[colon_pos + 1..].trim().to_string();

            match key.as_str() {
                "location" => location = Some(value),
                "st" => search_target = Some(value),
                "usn" => usn = Some(value),
                "server" => server = Some(value),
                _ => {}
            }
        }
    }

    if let (Some(loc), Some(st), Some(u)) = (location, search_target, usn) {
        Some(SsdpResponse {
            location: loc,
            search_target: st,
            usn: u,
            server: server.unwrap_or_else(|| "Unknown".to_string()),
        })
    } else {
        None
    }
}

/// 从 location URL 提取 IP 和端口
fn parse_location(location: &str) -> (String, u16) {
    // 简化解析：http://192.168.1.100:8080/description.xml
    if let Some(start) = location.find("://") {
        let rest = &location[start + 3..];
        if let Some(end) = rest.find('/') {
            let host_port = &rest[..end];
            if let Some(colon) = host_port.find(':') {
                let ip = host_port[..colon].to_string();
                let port = host_port[colon + 1..].parse().unwrap_or(80);
                return (ip, port);
            } else {
                return (host_port.to_string(), 80);
            }
        } else if let Some(colon) = rest.find(':') {
            let ip = rest[..colon].to_string();
            let port = rest[colon + 1..].parse().unwrap_or(80);
            return (ip, port);
        } else {
            return (rest.to_string(), 80);
        }
    }
    ("0.0.0.0".to_string(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSsdpSocket {
        responses: Vec<SsdpResponse>,
    }

    #[async_trait]
    impl SsdpSocket for MockSsdpSocket {
        async fn send_msearch(&self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn recv_responses(&self, _timeout_secs: u64) -> anyhow::Result<Vec<SsdpResponse>> {
            Ok(self.responses.clone())
        }
    }

    #[test]
    fn build_msearch_valid() {
        let request = build_msearch_request();
        let request_str = String::from_utf8(request).unwrap();

        assert!(request_str.contains("M-SEARCH * HTTP/1.1"));
        assert!(request_str.contains("HOST: 239.255.255.250:1900"));
        assert!(request_str.contains("MAN: \"ssdp:discover\""));
        assert!(request_str.contains("MX: 3"));
        assert!(request_str.contains("ST: ssdp:all"));
    }

    #[test]
    fn parse_ssdp_response_valid() {
        let response = "HTTP/1.1 200 OK\r\n\
                        CACHE-CONTROL: max-age=1800\r\n\
                        LOCATION: http://192.168.1.100:8080/description.xml\r\n\
                        ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
                        USN: uuid:12345678-1234-1234-1234-123456789012::urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
                        SERVER: Linux/3.0 UPnP/1.0 Device/1.0\r\n\
                        \r\n";

        let parsed = parse_ssdp_response(response);
        assert!(parsed.is_some());

        let resp = parsed.unwrap();
        assert_eq!(resp.location, "http://192.168.1.100:8080/description.xml");
        assert_eq!(
            resp.search_target,
            "urn:schemas-upnp-org:device:MediaRenderer:1"
        );
        assert!(resp.usn.contains("uuid:12345678"));
        assert_eq!(resp.server, "Linux/3.0 UPnP/1.0 Device/1.0");
    }

    #[test]
    fn parse_ssdp_response_missing_location() {
        let response = "HTTP/1.1 200 OK\r\n\
                        ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
                        USN: uuid:12345678-1234-1234-1234-123456789012\r\n\
                        \r\n";

        let parsed = parse_ssdp_response(response);
        assert!(parsed.is_none());
    }

    #[test]
    fn infer_from_urn_all_mappings() {
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn("urn:schemas-upnp-org:device:MediaRenderer:1"),
            "Speaker"
        );
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn("urn:schemas-upnp-org:device:MediaServer:1"),
            "Nas"
        );
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn(
                "urn:schemas-upnp-org:device:InternetGatewayDevice:1"
            ),
            "Computer"
        );
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn("urn:schemas-upnp-org:device:Basic:1"),
            "Sensor"
        );
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn("urn:schemas-upnp-org:device:DimmableLight:1"),
            "Light"
        );
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn("urn:schemas-upnp-org:device:BinaryLight:1"),
            "Light"
        );
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn("urn:schemas-upnp-org:device:HVAC:1"),
            "Thermostat"
        );
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn(
                "urn:schemas-upnp-org:device:DigitalSecurityCamera:1"
            ),
            "Camera"
        );
        assert_eq!(
            SsdpDiscoveryProvider::infer_from_urn("urn:schemas-upnp-org:device:Unknown:1"),
            "Sensor"
        );
    }

    #[tokio::test]
    async fn scan_with_mock_returns_devices() {
        let mock = MockSsdpSocket {
            responses: vec![
                SsdpResponse {
                    location: "http://192.168.1.10:8080/desc.xml".to_string(),
                    search_target: "urn:schemas-upnp-org:device:MediaRenderer:1".to_string(),
                    usn: "uuid:device1".to_string(),
                    server: "Speaker Device".to_string(),
                },
                SsdpResponse {
                    location: "http://192.168.1.20:9000/desc.xml".to_string(),
                    search_target: "urn:schemas-upnp-org:device:MediaServer:1".to_string(),
                    usn: "uuid:device2".to_string(),
                    server: "NAS Device".to_string(),
                },
            ],
        };

        let provider = SsdpDiscoveryProvider::with_socket(Box::new(mock));
        let devices = provider.scan(1).await.unwrap();

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].device_type, "Speaker");
        assert_eq!(devices[0].ip, "192.168.1.10");
        assert_eq!(devices[0].port, 8080);
        assert_eq!(devices[1].device_type, "Nas");
        assert_eq!(devices[1].ip, "192.168.1.20");
        assert_eq!(devices[1].port, 9000);
    }

    #[tokio::test]
    async fn scan_timeout_returns_collected() {
        let mock = MockSsdpSocket {
            responses: vec![SsdpResponse {
                location: "http://10.0.0.1:80/desc.xml".to_string(),
                search_target: "urn:schemas-upnp-org:device:Basic:1".to_string(),
                usn: "uuid:test".to_string(),
                server: "Test Device".to_string(),
            }],
        };

        let provider = SsdpDiscoveryProvider::with_socket(Box::new(mock));
        let devices = provider.scan(5).await.unwrap();

        assert_eq!(devices.len(), 1);
    }

    #[tokio::test]
    async fn scan_dedup_same_usn() {
        let mock = MockSsdpSocket {
            responses: vec![
                SsdpResponse {
                    location: "http://192.168.1.100:8080/desc.xml".to_string(),
                    search_target: "urn:schemas-upnp-org:device:MediaRenderer:1".to_string(),
                    usn: "uuid:same-device".to_string(),
                    server: "Device 1".to_string(),
                },
                SsdpResponse {
                    location: "http://192.168.1.100:8080/desc.xml".to_string(),
                    search_target: "urn:schemas-upnp-org:device:MediaRenderer:1".to_string(),
                    usn: "uuid:same-device".to_string(),
                    server: "Device 2".to_string(),
                },
            ],
        };

        let provider = SsdpDiscoveryProvider::with_socket(Box::new(mock));
        let devices = provider.scan(1).await.unwrap();

        assert_eq!(devices.len(), 1);
    }

    #[tokio::test]
    async fn scan_empty_response() {
        let mock = MockSsdpSocket {
            responses: Vec::new(),
        };

        let provider = SsdpDiscoveryProvider::with_socket(Box::new(mock));
        let devices = provider.scan(1).await.unwrap();

        assert!(devices.is_empty());
    }

    #[test]
    fn parse_ssdp_response_malformed() {
        let response = "This is not a valid SSDP response";
        let parsed = parse_ssdp_response(response);
        assert!(parsed.is_none());
    }

    #[test]
    fn is_available_returns_true() {
        let provider = SsdpDiscoveryProvider::new();
        assert!(provider.is_available());
    }
}
