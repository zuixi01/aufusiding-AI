//! 红外网关适配器
//!
//! 支持 Broadlink RM4 / Tuya IR / 小米红外网关。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::iot::control::DeviceController;

/// 红外网关类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrGatewayType {
    BroadlinkRm4,
    TuyaIr,
    XiaomiIr,
    Esp32Mqtt,
}

/// 红外桥接适配器
pub struct IrBridgeAdapter {
    gateway_ip: String,
    gateway_type: IrGatewayType,
    /// HTTP 网关 URL（可选，用于 HTTP 模式）
    gateway_http_url: Option<String>,
    /// 红外码存储（device_id -> command -> ir_data）
    code_store: std::collections::HashMap<String, std::collections::HashMap<String, Vec<u8>>>,
    /// 设备状态缓存
    states: std::sync::Mutex<std::collections::HashMap<String, Value>>,
}

impl IrBridgeAdapter {
    pub fn new(gateway_ip: String, gateway_type: IrGatewayType) -> Self {
        Self {
            gateway_ip,
            gateway_type,
            gateway_http_url: None,
            code_store: std::collections::HashMap::new(),
            states: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 创建 HTTP 模式的红外网关适配器
    pub fn with_http_gateway(gateway_ip: String, gateway_type: IrGatewayType, http_url: &str) -> Self {
        Self {
            gateway_ip,
            gateway_type,
            gateway_http_url: Some(http_url.to_string()),
            code_store: std::collections::HashMap::new(),
            states: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 网关 IP
    pub fn gateway_ip(&self) -> &str {
        &self.gateway_ip
    }

    /// 网关类型
    pub fn gateway_type(&self) -> &IrGatewayType {
        &self.gateway_type
    }

    /// 存储红外码
    pub fn store_code(&mut self, device_id: &str, command: &str, ir_data: Vec<u8>) {
        self.code_store
            .entry(device_id.to_string())
            .or_default()
            .insert(command.to_string(), ir_data);
    }

    /// 获取红外码
    pub fn get_code(&self, device_id: &str, command: &str) -> Option<&Vec<u8>> {
        self.code_store
            .get(device_id)
            .and_then(|cmds| cmds.get(command))
    }

    /// 列出设备的所有命令
    pub fn list_commands(&self, device_id: &str) -> Vec<String> {
        self.code_store
            .get(device_id)
            .map(|cmds| cmds.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 发送红外码
    async fn send_ir(&self, ir_data: &[u8]) -> anyhow::Result<()> {
        if let Some(ref url) = self.gateway_http_url {
            let client = reqwest::Client::new();
            let body = serde_json::json!({
                "type": format!("{:?}", self.gateway_type),
                "data": ir_data,
            });
            let resp = client
                .post(format!("{}/ir/send", url))
                .json(&body)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!("IR 网关 HTTP 发送失败: {}", resp.status());
            }
            return Ok(());
        }
        tracing::debug!(
            gateway = %self.gateway_ip,
            gateway_type = ?self.gateway_type,
            "发送红外码"
        );
        Ok(())
    }
}

#[async_trait]
impl DeviceController for IrBridgeAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let code = self.get_code(device_id, "power_on")
            .ok_or_else(|| anyhow::anyhow!("未找到 {} 的开机红外码", device_id))?;
        self.send_ir(code).await?;
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states.insert(device_id.to_string(), serde_json::json!({"powered": true}));
        Ok(())
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let code = self.get_code(device_id, "power_off")
            .ok_or_else(|| anyhow::anyhow!("未找到 {} 的关机红外码", device_id))?;
        self.send_ir(code).await?;
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states.insert(device_id.to_string(), serde_json::json!({"powered": false}));
        Ok(())
    }

    async fn set_property(&self, device_id: &str, property: &str, value: Value) -> anyhow::Result<()> {
        let command = format!("set_{}", property);
        let code = self.get_code(device_id, &command)
            .ok_or_else(|| anyhow::anyhow!("未找到 {} 的 {} 红外码", device_id, command))?;
        self.send_ir(code).await?;
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states.entry(device_id.to_string()).or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = state.as_object_mut() {
            obj.insert(property.to_string(), value);
        }
        Ok(())
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states
            .get(device_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("设备状态未知: {}", device_id))
    }

    fn supported_type(&self) -> &str {
        "infrared"
    }
}
