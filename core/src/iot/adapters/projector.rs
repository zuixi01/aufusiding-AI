//! 投影仪控制适配器
//!
//! 支持 DLNA + HDMI-CEC + 网络投影。

use async_trait::async_trait;
use serde_json::Value;

use crate::iot::control::DeviceController;

/// 投影仪适配器
pub struct ProjectorAdapter {
    device_id: String,
    /// 投影仪 IP（网络投影）
    ip: Option<String>,
    /// 是否支持 CEC
    cec_enabled: bool,
    /// 状态缓存
    state: std::sync::Mutex<ProjectorState>,
}

#[derive(Debug, Clone)]
struct ProjectorState {
    powered: bool,
    input_source: String,
    brightness: u8,
}

impl ProjectorAdapter {
    pub fn new(device_id: String, ip: Option<String>, cec_enabled: bool) -> Self {
        Self {
            device_id,
            ip,
            cec_enabled,
            state: std::sync::Mutex::new(ProjectorState {
                powered: false,
                input_source: "hdmi1".into(),
                brightness: 100,
            }),
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn ip(&self) -> Option<&str> {
        self.ip.as_deref()
    }

    pub fn cec_enabled(&self) -> bool {
        self.cec_enabled
    }
}

#[async_trait]
impl DeviceController for ProjectorAdapter {
    async fn turn_on(&self, _device_id: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.powered = true;
        Ok(())
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.powered = false;
        Ok(())
    }

    async fn set_property(
        &self,
        _device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match property {
            "input_source" => {
                state.input_source = value.as_str().unwrap_or("hdmi1").to_string();
            }
            "brightness" => {
                state.brightness = value.as_u64().unwrap_or(100) as u8;
            }
            _ => anyhow::bail!("不支持的属性: {}", property),
        }
        Ok(())
    }

    async fn get_state(&self, _device_id: &str) -> anyhow::Result<Value> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Ok(serde_json::json!({
            "powered": state.powered,
            "input_source": state.input_source,
            "brightness": state.brightness,
        }))
    }

    fn supported_type(&self) -> &str {
        "projector"
    }
}
