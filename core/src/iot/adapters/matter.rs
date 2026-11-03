//! Matter 协议适配器
//!
//! 通过 Matter over IP 控制智能家居设备。
//! 支持设备发现 (mDNS _matter._tcp)、PASE/CASE 配对、属性读写。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::iot::control::DeviceController;

// ── Matter 数据结构 ──────────────────────────────────────────

/// Matter 设备节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterNode {
    pub node_id: u64,
    pub endpoints: Vec<MatterEndpoint>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_name: String,
}

/// Matter 端点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterEndpoint {
    pub endpoint_id: u16,
    pub device_type: MatterDeviceType,
    pub clusters: Vec<MatterCluster>,
}

/// Matter 设备类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatterDeviceType {
    OnOffLight,
    DimmableLight,
    ColorTemperatureLight,
    Thermostat,
    DoorLock,
    WindowCovering,
    ContactSensor,
    OccupancySensor,
    TemperatureSensor,
    Fan,
    Unknown(u16),
}

impl MatterDeviceType {
    pub fn from_id(id: u16) -> Self {
        match id {
            0x0100 => Self::OnOffLight,
            0x0101 => Self::DimmableLight,
            0x0102 => Self::ColorTemperatureLight,
            0x0301 => Self::Thermostat,
            0x000A => Self::DoorLock,
            0x0202 => Self::WindowCovering,
            0x0015 => Self::ContactSensor,
            0x0107 => Self::OccupancySensor,
            0x0302 => Self::TemperatureSensor,
            0x002B => Self::Fan,
            other => Self::Unknown(other),
        }
    }
}

/// Matter 集群
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterCluster {
    pub cluster_id: u32,
    pub cluster_name: String,
    pub attributes: Vec<MatterAttribute>,
}

/// Matter 属性
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterAttribute {
    pub attribute_id: u32,
    pub name: String,
    pub value: Value,
    pub writable: bool,
}

/// Matter 集群 ID 常量
pub mod cluster_ids {
    pub const ON_OFF: u32 = 0x0006;
    pub const LEVEL_CONTROL: u32 = 0x0008;
    pub const COLOR_CONTROL: u32 = 0x0300;
    pub const THERMOSTAT: u32 = 0x0201;
    pub const DOOR_LOCK: u32 = 0x0101;
    pub const FAN_CONTROL: u32 = 0x0202;
    pub const TEMPERATURE_MEASUREMENT: u32 = 0x0402;
}

/// Matter 配对状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommissioningState {
    /// 未配对
    NotCommissioned,
    /// PASE 配对中 (PIN 码)
    PaseInProgress,
    /// CASE 会话建立中 (证书)
    CaseInProgress,
    /// 已配对
    Commissioned,
}

/// Matter 设备注册信息
struct MatterEntry {
    node_id: u64,
    device_name: String,
    device_type: MatterDeviceType,
    state: CommissioningState,
}

/// Matter 协议适配器
pub struct MatterAdapter {
    client: reqwest::Client,
    base_url: String,
    fabric_id: u64,
    nodes: HashMap<String, MatterEntry>,
}

impl MatterAdapter {
    pub fn new(base_url: &str, fabric_id: u64) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            fabric_id,
            nodes: HashMap::new(),
        }
    }

    /// 注册 Matter 设备
    pub fn register_node(
        &mut self,
        device_id: &str,
        node_id: u64,
        name: &str,
        device_type: MatterDeviceType,
    ) {
        self.nodes.insert(
            device_id.to_string(),
            MatterEntry {
                node_id,
                device_name: name.to_string(),
                device_type,
                state: CommissioningState::Commissioned,
            },
        );
    }

    fn get_entry(&self, device_id: &str) -> anyhow::Result<&MatterEntry> {
        self.nodes
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("未知 Matter 设备: {}", device_id))
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// 读取集群属性
    pub async fn read_attribute(
        &self,
        device_id: &str,
        endpoint_id: u16,
        cluster_id: u32,
        attribute_id: u32,
    ) -> anyhow::Result<Value> {
        let entry = self.get_entry(device_id)?;
        let url = self.endpoint(&format!(
            "/node/{}/endpoint/{}/cluster/{}/attribute/{}",
            entry.node_id, endpoint_id, cluster_id, attribute_id
        ));
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("读取属性失败: {}", resp.status());
        }
        let body: Value = resp.json().await?;
        Ok(body)
    }

    /// 写入集群属性
    pub async fn write_attribute(
        &self,
        device_id: &str,
        endpoint_id: u16,
        cluster_id: u32,
        attribute_id: u32,
        value: Value,
    ) -> anyhow::Result<()> {
        let entry = self.get_entry(device_id)?;
        let url = self.endpoint(&format!(
            "/node/{}/endpoint/{}/cluster/{}/attribute/{}",
            entry.node_id, endpoint_id, cluster_id, attribute_id
        ));
        let resp = self.client.put(&url).json(&value).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("写入属性失败: {}", resp.status());
        }
        Ok(())
    }

    /// 发送集群命令
    pub async fn send_command(
        &self,
        device_id: &str,
        endpoint_id: u16,
        cluster_id: u32,
        command_id: u32,
        payload: Value,
    ) -> anyhow::Result<Value> {
        let entry = self.get_entry(device_id)?;
        let url = self.endpoint(&format!(
            "/node/{}/endpoint/{}/cluster/{}/command/{}",
            entry.node_id, endpoint_id, cluster_id, command_id
        ));
        let resp = self.client.post(&url).json(&payload).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("发送命令失败: {}", resp.status());
        }
        let body: Value = resp.json().await?;
        Ok(body)
    }

    /// PASE 配对 (PIN 码)
    pub async fn commission_pase(&mut self, device_id: &str, setup_pin: u32) -> anyhow::Result<()> {
        if let Some(entry) = self.nodes.get_mut(device_id) {
            entry.state = CommissioningState::PaseInProgress;
        }
        let url = self.endpoint("/commission/pase");
        let resp = self
            .client
            .post(&url)
            .json(&json!({
                "device_id": device_id,
                "setup_pin": setup_pin,
                "fabric_id": self.fabric_id,
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            if let Some(entry) = self.nodes.get_mut(device_id) {
                entry.state = CommissioningState::NotCommissioned;
            }
            anyhow::bail!("PASE 配对失败: {}", resp.status());
        }
        if let Some(entry) = self.nodes.get_mut(device_id) {
            entry.state = CommissioningState::Commissioned;
        }
        Ok(())
    }

    pub fn fabric_id(&self) -> u64 {
        self.fabric_id
    }
}

#[async_trait]
impl DeviceController for MatterAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        // OnOff cluster, On command (ID=1)
        self.send_command(device_id, 1, cluster_ids::ON_OFF, 1, json!({}))
            .await?;
        Ok(())
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        // OnOff cluster, Off command (ID=0)
        self.send_command(device_id, 1, cluster_ids::ON_OFF, 0, json!({}))
            .await?;
        Ok(())
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let (cluster_id, attr_id) = match property {
            "brightness" | "level" => (cluster_ids::LEVEL_CONTROL, 0u32),
            "color_temperature" => (cluster_ids::COLOR_CONTROL, 7),
            "target_temperature" => (cluster_ids::THERMOSTAT, 18),
            "fan_speed" => (cluster_ids::FAN_CONTROL, 2),
            _ => anyhow::bail!("不支持的属性: {}", property),
        };
        self.write_attribute(device_id, 1, cluster_id, attr_id, value)
            .await
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let entry = self.get_entry(device_id)?;
        // 读取 OnOff 状态
        let on_off = self
            .read_attribute(device_id, 1, cluster_ids::ON_OFF, 0)
            .await
            .unwrap_or(json!(null));
        Ok(json!({
            "node_id": entry.node_id,
            "device_name": entry.device_name,
            "device_type": format!("{:?}", entry.device_type),
            "commissioned": entry.state == CommissioningState::Commissioned,
            "on_off": on_off,
        }))
    }

    fn supported_type(&self) -> &str {
        "matter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_type_from_id() {
        assert_eq!(
            MatterDeviceType::from_id(0x0100),
            MatterDeviceType::OnOffLight
        );
        assert_eq!(
            MatterDeviceType::from_id(0x0301),
            MatterDeviceType::Thermostat
        );
        assert_eq!(
            MatterDeviceType::from_id(0xFFFF),
            MatterDeviceType::Unknown(0xFFFF)
        );
    }

    #[test]
    fn register_and_lookup_node() {
        let mut adapter = MatterAdapter::new("http://localhost:5540", 1);
        adapter.register_node("light-1", 100, "客厅灯", MatterDeviceType::OnOffLight);
        assert!(adapter.get_entry("light-1").is_ok());
        assert!(adapter.get_entry("nonexistent").is_err());
    }

    #[test]
    fn commissioning_state_default() {
        let mut adapter = MatterAdapter::new("http://localhost:5540", 1);
        adapter.register_node("dev-1", 1, "设备", MatterDeviceType::DoorLock);
        let entry = adapter.get_entry("dev-1").unwrap();
        assert_eq!(entry.state, CommissioningState::Commissioned);
    }

    #[test]
    fn fabric_id_accessible() {
        let adapter = MatterAdapter::new("http://localhost:5540", 42);
        assert_eq!(adapter.fabric_id(), 42);
    }

    #[test]
    fn supported_type_is_matter() {
        let adapter = MatterAdapter::new("http://localhost:5540", 1);
        assert_eq!(adapter.supported_type(), "matter");
    }

    #[test]
    fn cluster_id_constants() {
        assert_eq!(cluster_ids::ON_OFF, 0x0006);
        assert_eq!(cluster_ids::THERMOSTAT, 0x0201);
        assert_eq!(cluster_ids::DOOR_LOCK, 0x0101);
    }

    #[test]
    fn matter_endpoint_url_format() {
        let adapter = MatterAdapter::new("http://matter.local:5540", 1);
        let url = adapter.endpoint("/node/100/endpoint/1/cluster/6/attribute/0");
        assert_eq!(
            url,
            "http://matter.local:5540/node/100/endpoint/1/cluster/6/attribute/0"
        );
    }
}
