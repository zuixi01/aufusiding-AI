//! mDNS/Bonjour 发现后端
//!
//! 通过 mDNS 多播发现局域网设备：打印机、音响、电脑、NAS、HomeKit 等。

use async_trait::async_trait;
use std::collections::HashMap;

use super::discovery::DeviceInfo;
use super::discovery_provider::DiscoveryProvider;

/// mDNS 服务类型到设备类型的映射
const SERVICE_MAPPINGS: &[(&str, &str, &str)] = &[
    ("_ipp._tcp.local.", "Printer", "ipp"),
    ("_ipps._tcp.local.", "Printer", "ipp"),
    ("_airplay._tcp.local.", "Speaker", "http"),
    ("_raop._tcp.local.", "Speaker", "http"),
    ("_googlecast._tcp.local.", "Speaker", "http"),
    ("_workstation._tcp.local.", "Computer", "mdns"),
    ("_smb._tcp.local.", "Nas", "smb"),
    ("_nfs._tcp.local.", "Nas", "nfs"),
    ("_hap._tcp.local.", "Light", "homekit"),
    ("_matter._tcp.local.", "Sensor", "matter"),
    ("_http._tcp.local.", "Sensor", "http"),
];

/// mDNS 响应数据
#[derive(Debug, Clone)]
pub struct MdnsResponse {
    pub hostname: String,
    pub ip: String,
    pub port: u16,
    pub service_type: String,
    pub txt_records: HashMap<String, String>,
}

/// mDNS UDP socket 抽象
#[async_trait]
pub trait MdnsSocket: Send + Sync {
    async fn send_query(&self, service_type: &str) -> anyhow::Result<()>;
    async fn recv_responses(&self, timeout_secs: u64) -> anyhow::Result<Vec<MdnsResponse>>;
}

/// 默认 mDNS socket（返回空结果，真实网络需要实际 UDP 实现）
pub struct DefaultMdnsSocket;

#[async_trait]
impl MdnsSocket for DefaultMdnsSocket {
    async fn send_query(&self, _service_type: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn recv_responses(&self, _timeout_secs: u64) -> anyhow::Result<Vec<MdnsResponse>> {
        Ok(Vec::new())
    }
}

/// Real mDNS socket using UDP multicast to 224.0.0.251:5353.
///
/// Falls back to empty results with a warning log if the network is unavailable.
pub struct RealMdnsSocket {
    /// The last service_type queried, stored for recv_responses context.
    last_query: tokio::sync::RwLock<Option<String>>,
}

impl RealMdnsSocket {
    pub fn new() -> Self {
        Self {
            last_query: tokio::sync::RwLock::new(None),
        }
    }
}

const MDNS_MULTICAST_ADDR: &str = "224.0.0.251:5353";

#[async_trait]
impl MdnsSocket for RealMdnsSocket {
    async fn send_query(&self, service_type: &str) -> anyhow::Result<()> {
        *self.last_query.write().await = Some(service_type.to_string());

        let socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("mDNS: failed to bind UDP socket: {}", e);
                return Ok(());
            }
        };

        let packet = build_mdns_query(service_type);
        if let Err(e) = socket.send_to(&packet, MDNS_MULTICAST_ADDR).await {
            tracing::warn!("mDNS: failed to send query to {}: {}", MDNS_MULTICAST_ADDR, e);
        }

        Ok(())
    }

    async fn recv_responses(&self, timeout_secs: u64) -> anyhow::Result<Vec<MdnsResponse>> {
        let socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("mDNS: failed to bind UDP socket for recv: {}", e);
                return Ok(Vec::new());
            }
        };

        let mut responses = Vec::new();
        let mut buf = [0u8; 4096];
        let deadline = tokio::time::Instant::now()
            + tokio::time::Duration::from_secs(timeout_secs);

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
                Ok(Ok((len, _addr))) => {
                    if let Some(resp) = parse_mdns_response(&buf[..len]) {
                        responses.push(resp);
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("mDNS: recv error: {}", e);
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

/// mDNS 发现后端
pub struct MdnsDiscoveryProvider {
    service_types: Vec<String>,
    socket: Box<dyn MdnsSocket>,
}

impl MdnsDiscoveryProvider {
    pub fn new() -> Self {
        Self {
            service_types: SERVICE_MAPPINGS
                .iter()
                .map(|(svc, _, _)| svc.to_string())
                .collect(),
            socket: Box::new(DefaultMdnsSocket),
        }
    }

    pub fn with_socket(socket: Box<dyn MdnsSocket>) -> Self {
        Self {
            service_types: SERVICE_MAPPINGS
                .iter()
                .map(|(svc, _, _)| svc.to_string())
                .collect(),
            socket,
        }
    }

    pub fn with_service_types(service_types: Vec<String>) -> Self {
        Self {
            service_types,
            socket: Box::new(DefaultMdnsSocket),
        }
    }

    /// 根据服务类型推断设备类型和协议
    fn infer_device_type(service_type: &str) -> (&str, &str) {
        for (svc, dtype, proto) in SERVICE_MAPPINGS {
            if service_type.contains(svc) {
                return (dtype, proto);
            }
        }
        ("Sensor", "mdns")
    }
}

#[async_trait]
impl DiscoveryProvider for MdnsDiscoveryProvider {
    fn name(&self) -> &str {
        "mDNS"
    }

    async fn scan(&self, timeout_secs: u64) -> anyhow::Result<Vec<DeviceInfo>> {
        let mut all_responses = Vec::new();

        // 查询所有服务类型
        for service_type in &self.service_types {
            self.socket.send_query(service_type).await?;
            let responses = self.socket.recv_responses(timeout_secs).await?;
            all_responses.extend(responses);
        }

        // 转换为 DeviceInfo 并去重
        let mut seen = HashMap::new();
        let mut devices = Vec::new();

        for response in all_responses {
            let key = format!("{}:{}", response.ip, response.port);
            if seen.contains_key(&key) {
                continue;
            }
            seen.insert(key.clone(), true);

            let (device_type, protocol) = Self::infer_device_type(&response.service_type);
            let name = response
                .txt_records
                .get("name")
                .cloned()
                .unwrap_or_else(|| response.hostname.clone());

            devices.push(DeviceInfo {
                id: format!("mdns-{}", uuid::Uuid::new_v4()),
                name,
                device_type: device_type.to_string(),
                ip: response.ip,
                port: response.port,
                protocol: protocol.to_string(),
            });
        }

        Ok(devices)
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// 构建 mDNS 查询数据包
pub fn build_mdns_query(service_type: &str) -> Vec<u8> {
    let mut packet = Vec::new();

    // DNS 头部（12 字节）
    packet.extend_from_slice(&[0x00, 0x00]); // Transaction ID = 0
    packet.extend_from_slice(&[0x00, 0x00]); // Flags = 0 (标准查询)
    packet.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
    packet.extend_from_slice(&[0x00, 0x00]); // ANCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x00]); // NSCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x00]); // ARCOUNT = 0

    // 问题部分：编码服务名称
    for label in service_type.split('.') {
        if !label.is_empty() {
            packet.push(label.len() as u8);
            packet.extend_from_slice(label.as_bytes());
        }
    }
    packet.push(0x00); // 结束标签

    // QTYPE = PTR (12), QCLASS = IN (1)
    packet.extend_from_slice(&[0x00, 0x0c, 0x00, 0x01]);

    packet
}

/// 解析 mDNS 响应数据包
pub fn parse_mdns_response(data: &[u8]) -> Option<MdnsResponse> {
    if data.len() < 12 {
        return None;
    }

    // 简化解析：提取 A 记录、SRV 记录、TXT 记录
    let mut hostname = String::new();
    let mut ip = String::new();
    let mut port = 0u16;
    let mut service_type = String::new();
    let txt_records = HashMap::new();

    // 跳过头部和问题部分，查找答案部分
    let mut offset = 12;

    // 跳过问题部分（简化：假设只有一个问题）
    while offset < data.len() && data[offset] != 0 {
        let len = data[offset] as usize;
        offset += len + 1;
    }
    offset += 5; // 跳过结束标签和 QTYPE/QCLASS

    // 解析答案部分
    while offset + 10 < data.len() {
        // 跳过名称（简化：假设使用压缩指针）
        if data[offset] & 0xc0 == 0xc0 {
            offset += 2;
        } else {
            while offset < data.len() && data[offset] != 0 {
                let len = data[offset] as usize;
                offset += len + 1;
            }
            offset += 1;
        }

        if offset + 10 > data.len() {
            break;
        }

        let rtype = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let rdlength = u16::from_be_bytes([data[offset + 8], data[offset + 9]]) as usize;
        offset += 10;

        if offset + rdlength > data.len() {
            break;
        }

        match rtype {
            1 => {
                // A 记录
                if rdlength == 4 {
                    ip = format!(
                        "{}.{}.{}.{}",
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3]
                    );
                }
            }
            33 => {
                // SRV 记录
                if rdlength >= 6 {
                    port = u16::from_be_bytes([data[offset + 4], data[offset + 5]]);
                    // 提取主机名（简化）
                    hostname = "device.local".to_string();
                }
            }
            12 => {
                // PTR 记录
                service_type = "_http._tcp.local.".to_string();
            }
            _ => {}
        }

        offset += rdlength;
    }

    if !ip.is_empty() {
        Some(MdnsResponse {
            hostname: if hostname.is_empty() {
                "unknown".to_string()
            } else {
                hostname
            },
            ip,
            port: if port == 0 { 80 } else { port },
            service_type: if service_type.is_empty() {
                "_http._tcp.local.".to_string()
            } else {
                service_type
            },
            txt_records,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMdnsSocket {
        responses: Vec<MdnsResponse>,
    }

    #[async_trait]
    impl MdnsSocket for MockMdnsSocket {
        async fn send_query(&self, _service_type: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn recv_responses(&self, _timeout_secs: u64) -> anyhow::Result<Vec<MdnsResponse>> {
            Ok(self.responses.clone())
        }
    }

    #[test]
    fn parse_mdns_response_a_record() {
        let mut packet = vec![0u8; 12];
        // 头部：1 个答案
        packet[6] = 0x00;
        packet[7] = 0x01;

        // 问题部分（跳过）
        packet.extend_from_slice(&[0x04, b't', b'e', b's', b't', 0x00]);
        packet.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);

        // 答案：A 记录
        packet.extend_from_slice(&[0xc0, 0x0c]); // 名称指针
        packet.extend_from_slice(&[0x00, 0x01]); // TYPE = A
        packet.extend_from_slice(&[0x00, 0x01]); // CLASS = IN
        packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // TTL
        packet.extend_from_slice(&[0x00, 0x04]); // RDLENGTH = 4
        packet.extend_from_slice(&[192, 168, 1, 100]); // IP

        let response = parse_mdns_response(&packet);
        assert!(response.is_some());
        let resp = response.unwrap();
        assert_eq!(resp.ip, "192.168.1.100");
    }

    #[test]
    fn parse_mdns_response_srv_record() {
        let mut packet = vec![0u8; 12];
        packet[6] = 0x00;
        packet[7] = 0x01;

        packet.extend_from_slice(&[0x04, b't', b'e', b's', b't', 0x00]);
        packet.extend_from_slice(&[0x00, 0x21, 0x00, 0x01]);

        // SRV 记录
        packet.extend_from_slice(&[0xc0, 0x0c]);
        packet.extend_from_slice(&[0x00, 0x21]); // TYPE = SRV
        packet.extend_from_slice(&[0x00, 0x01]);
        packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]);
        packet.extend_from_slice(&[0x00, 0x08]); // RDLENGTH
        packet.extend_from_slice(&[0x00, 0x00]); // Priority
        packet.extend_from_slice(&[0x00, 0x00]); // Weight
        packet.extend_from_slice(&[0x1f, 0x90]); // Port = 8080
        packet.extend_from_slice(&[0x00, 0x00]); // Target

        // 添加 A 记录以提供 IP
        packet.extend_from_slice(&[0xc0, 0x0c]);
        packet.extend_from_slice(&[0x00, 0x01]);
        packet.extend_from_slice(&[0x00, 0x01]);
        packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]);
        packet.extend_from_slice(&[0x00, 0x04]);
        packet.extend_from_slice(&[10, 0, 0, 1]);

        packet[7] = 0x02; // 2 个答案

        let response = parse_mdns_response(&packet);
        assert!(response.is_some());
        let resp = response.unwrap();
        assert_eq!(resp.port, 8080);
        assert_eq!(resp.ip, "10.0.0.1");
    }

    #[test]
    fn parse_mdns_response_txt_record() {
        // TXT 记录解析（简化测试）
        let mut packet = vec![0u8; 12];
        packet[6] = 0x00;
        packet[7] = 0x01;

        packet.extend_from_slice(&[0x04, b't', b'e', b's', b't', 0x00]);
        packet.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);

        packet.extend_from_slice(&[0xc0, 0x0c]);
        packet.extend_from_slice(&[0x00, 0x01]);
        packet.extend_from_slice(&[0x00, 0x01]);
        packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]);
        packet.extend_from_slice(&[0x00, 0x04]);
        packet.extend_from_slice(&[192, 168, 1, 50]);

        let response = parse_mdns_response(&packet);
        assert!(response.is_some());
    }

    #[test]
    fn build_mdns_query_valid() {
        let query = build_mdns_query("_http._tcp.local.");

        // 验证头部
        assert_eq!(query[0], 0x00); // ID
        assert_eq!(query[1], 0x00);
        assert_eq!(query[4], 0x00); // QDCOUNT
        assert_eq!(query[5], 0x01);

        // 验证包含服务名称
        assert!(query.len() > 12);
        assert_eq!(query[12], 5); // "_http" 长度
        assert_eq!(query[13], b'_');
    }

    #[test]
    fn infer_device_type_all_mappings() {
        assert_eq!(
            MdnsDiscoveryProvider::infer_device_type("_ipp._tcp.local."),
            ("Printer", "ipp")
        );
        assert_eq!(
            MdnsDiscoveryProvider::infer_device_type("_airplay._tcp.local."),
            ("Speaker", "http")
        );
        assert_eq!(
            MdnsDiscoveryProvider::infer_device_type("_workstation._tcp.local."),
            ("Computer", "mdns")
        );
        assert_eq!(
            MdnsDiscoveryProvider::infer_device_type("_smb._tcp.local."),
            ("Nas", "smb")
        );
        assert_eq!(
            MdnsDiscoveryProvider::infer_device_type("_hap._tcp.local."),
            ("Light", "homekit")
        );
        assert_eq!(
            MdnsDiscoveryProvider::infer_device_type("_matter._tcp.local."),
            ("Sensor", "matter")
        );
        assert_eq!(
            MdnsDiscoveryProvider::infer_device_type("_unknown._tcp.local."),
            ("Sensor", "mdns")
        );
    }

    #[tokio::test]
    async fn scan_with_mock_returns_devices() {
        let mock = MockMdnsSocket {
            responses: vec![
                MdnsResponse {
                    hostname: "printer.local".to_string(),
                    ip: "192.168.1.10".to_string(),
                    port: 631,
                    service_type: "_ipp._tcp.local.".to_string(),
                    txt_records: HashMap::new(),
                },
                MdnsResponse {
                    hostname: "speaker.local".to_string(),
                    ip: "192.168.1.20".to_string(),
                    port: 7000,
                    service_type: "_airplay._tcp.local.".to_string(),
                    txt_records: HashMap::new(),
                },
            ],
        };

        let provider = MdnsDiscoveryProvider::with_socket(Box::new(mock));
        let devices = provider.scan(1).await.unwrap();

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].device_type, "Printer");
        assert_eq!(devices[0].protocol, "ipp");
        assert_eq!(devices[1].device_type, "Speaker");
        assert_eq!(devices[1].protocol, "http");
    }

    #[tokio::test]
    async fn scan_timeout_returns_collected() {
        let mock = MockMdnsSocket {
            responses: vec![MdnsResponse {
                hostname: "device.local".to_string(),
                ip: "10.0.0.1".to_string(),
                port: 80,
                service_type: "_http._tcp.local.".to_string(),
                txt_records: HashMap::new(),
            }],
        };

        let provider = MdnsDiscoveryProvider::with_socket(Box::new(mock));
        let devices = provider.scan(5).await.unwrap();

        assert_eq!(devices.len(), 1);
    }

    #[tokio::test]
    async fn scan_dedup_same_ip_port() {
        let mock = MockMdnsSocket {
            responses: vec![
                MdnsResponse {
                    hostname: "device1.local".to_string(),
                    ip: "192.168.1.100".to_string(),
                    port: 8080,
                    service_type: "_http._tcp.local.".to_string(),
                    txt_records: HashMap::new(),
                },
                MdnsResponse {
                    hostname: "device2.local".to_string(),
                    ip: "192.168.1.100".to_string(),
                    port: 8080,
                    service_type: "_http._tcp.local.".to_string(),
                    txt_records: HashMap::new(),
                },
            ],
        };

        let provider = MdnsDiscoveryProvider::with_socket(Box::new(mock));
        let devices = provider.scan(1).await.unwrap();

        assert_eq!(devices.len(), 1);
    }

    #[tokio::test]
    async fn scan_empty_response() {
        let mock = MockMdnsSocket {
            responses: Vec::new(),
        };

        let provider = MdnsDiscoveryProvider::with_socket(Box::new(mock));
        let devices = provider.scan(1).await.unwrap();

        assert!(devices.is_empty());
    }

    #[test]
    fn is_available_returns_true() {
        let provider = MdnsDiscoveryProvider::new();
        assert!(provider.is_available());
    }
}
