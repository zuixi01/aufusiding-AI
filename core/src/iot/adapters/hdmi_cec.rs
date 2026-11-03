//! HDMI-CEC 设备控制
//!
//! 电视/投影仪开关、输入源切换。

use async_trait::async_trait;
use serde_json::Value;

use crate::iot::control::DeviceController;

/// CEC 命令
#[derive(Debug, Clone, Copy)]
pub enum CecCommand {
    PowerOn,
    PowerOff,
    VolumeUp,
    VolumeDown,
    Mute,
    SetActiveSource,
}

/// HDMI-CEC 适配器
pub struct HdmiCecAdapter {
    /// CEC 设备路径（如 /dev/cec0）
    device_path: String,
    /// 设备状态缓存
    states: std::sync::Mutex<std::collections::HashMap<String, Value>>,
}

impl HdmiCecAdapter {
    pub fn new(device_path: String) -> Self {
        Self {
            device_path,
            states: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn device_path(&self) -> &str {
        &self.device_path
    }

    /// 发送 CEC 命令
    async fn send_command(&self, _device_id: &str, cmd: CecCommand) -> anyhow::Result<()> {
        let cec_arg = match cmd {
            CecCommand::PowerOn => "on 0",
            CecCommand::PowerOff => "standby 0",
            CecCommand::VolumeUp => "volup",
            CecCommand::VolumeDown => "voldown",
            CecCommand::Mute => "mute",
            CecCommand::SetActiveSource => "as",
        };
        let output = tokio::process::Command::new("cec-client")
            .args(["-s", "-d", "1", &self.device_path])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();
        match output {
            Ok(mut child) => {
                if let Some(ref mut stdin) = child.stdin {
                    use tokio::io::AsyncWriteExt;
                    let _ = stdin
                        .write_all(format!("{}\nq\n", cec_arg).as_bytes())
                        .await;
                }
                let _ = child.wait().await;
            }
            Err(e) => {
                tracing::warn!(path = %self.device_path, error = %e, "cec-client 不可用，跳过 CEC 命令");
            }
        }
        Ok(())
    }
}

#[async_trait]
impl DeviceController for HdmiCecAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, CecCommand::PowerOn).await?;
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states.insert(device_id.to_string(), serde_json::json!({"powered": true}));
        Ok(())
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, CecCommand::PowerOff).await?;
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states.insert(device_id.to_string(), serde_json::json!({"powered": false}));
        Ok(())
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        match property {
            "volume" => {
                let _level = value.as_u64().unwrap_or(50);
                // 实际会发送多次 VolumeUp/Down
            }
            "active_source" => {
                self.send_command(device_id, CecCommand::SetActiveSource)
                    .await?;
            }
            _ => anyhow::bail!("CEC 不支持属性: {}", property),
        }
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states
            .entry(device_id.to_string())
            .or_insert_with(|| serde_json::json!({}));
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
            .ok_or_else(|| anyhow::anyhow!("CEC 设备状态未知: {}", device_id))
    }

    fn supported_type(&self) -> &str {
        "hdmi_cec"
    }
}
