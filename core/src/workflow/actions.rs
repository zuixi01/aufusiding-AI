//! 工作流动作
//!
//! 定义工作流触发后执行的动作（发送消息、控制设备等）。

use crate::iot::GenericController;
use crate::safety::permissions::Permission;
use crate::safety::permissions::PermissionManager;
use crate::screen::ScreenActionGraph;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub struct ActionExecutionContext<'a> {
    pub iot_controller: Option<&'a GenericController>,
    pub permission_manager: Option<&'a PermissionManager>,
}

impl<'a> ActionExecutionContext<'a> {
    pub fn new() -> Self {
        Self {
            iot_controller: None,
            permission_manager: None,
        }
    }

    pub fn with_iot_controller(mut self, controller: &'a GenericController) -> Self {
        self.iot_controller = Some(controller);
        self
    }

    pub fn with_permission_manager(mut self, permission_manager: &'a PermissionManager) -> Self {
        self.permission_manager = Some(permission_manager);
        self
    }
}

impl Default for ActionExecutionContext<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// 动作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// 发送通知
    SendNotification { title: String, body: String },
    /// 控制 IoT 设备
    ControlDevice {
        device_id: String,
        command: String,
        params: Value,
    },
    /// 执行 AI 推理
    RunInference { prompt: String },
    /// 发送消息到渠道
    SendMessage { channel: String, message: String },
    /// 执行脚本
    RunScript { script_path: String },
    /// HTTP 请求
    HttpRequest {
        url: String,
        method: String,
        body: Option<String>,
    },
    /// 校验屏幕动作计划
    ValidateScreenPlan { graph: ScreenActionGraph },
    /// 车辆命令
    VehicleCommand {
        device_id: String,
        command: String,
        params: Value,
    },
}

impl Action {
    /// 执行动作
    pub async fn execute(&self) -> anyhow::Result<()> {
        self.execute_with_context(&ActionExecutionContext::default())
            .await
            .map(|_| ())
    }

    /// 带执行上下文执行动作
    pub async fn execute_with_context(
        &self,
        context: &ActionExecutionContext<'_>,
    ) -> anyhow::Result<Value> {
        self.validate()?;

        match self {
            Action::SendNotification { title, body } => Ok(json!({
                "status": "ok",
                "action": "send_notification",
                "title": title,
                "body": body
            })),
            Action::ControlDevice {
                device_id,
                command,
                params,
            } => {
                let permission_manager = context
                    .permission_manager
                    .ok_or_else(|| anyhow::anyhow!("permission manager not configured"))?;
                let permission = Permission::ControlDevice(device_id.clone());
                if !permission_manager.check(&permission) {
                    anyhow::bail!("permission denied for device: {device_id}");
                }

                let controller = context
                    .iot_controller
                    .ok_or_else(|| anyhow::anyhow!("iot controller not configured"))?;
                controller.execute(device_id, command, params.clone()).await
            }
            Action::RunInference { prompt } => Ok(json!({
                "status": "ok",
                "action": "run_inference",
                "prompt": prompt
            })),
            Action::SendMessage { channel, message } => Ok(json!({
                "status": "ok",
                "action": "send_message",
                "channel": channel,
                "message": message
            })),
            Action::RunScript { script_path } => Ok(json!({
                "status": "ok",
                "action": "run_script",
                "script_path": script_path
            })),
            Action::HttpRequest { url, method, body } => Ok(json!({
                "status": "ok",
                "action": "http_request",
                "url": url,
                "method": method,
                "body": body
            })),
            Action::ValidateScreenPlan { graph } => {
                let permission_manager = context
                    .permission_manager
                    .ok_or_else(|| anyhow::anyhow!("permission manager not configured"))?;
                graph.validate(permission_manager)?;
                let ordered = graph.topological_order()?;
                Ok(json!({
                    "status": "ok",
                    "action": "validate_screen_plan",
                    "node_count": graph.nodes().len(),
                    "edge_count": graph.edges().len(),
                    "execution_order": ordered.iter().map(|node| node.id.to_string()).collect::<Vec<_>>()
                }))
            }
            Action::VehicleCommand {
                device_id,
                command,
                params,
            } => Ok(json!({
                "status": "ok",
                "action": "vehicle_command",
                "device_id": device_id,
                "command": command,
                "params": params
            })),
        }
    }

    /// 验证动作配置
    pub fn validate(&self) -> anyhow::Result<()> {
        fn require_non_empty(field_name: &str, value: &str) -> anyhow::Result<()> {
            if value.trim().is_empty() {
                anyhow::bail!("{field_name} cannot be empty");
            }
            Ok(())
        }

        match self {
            Action::SendNotification { title, body } => {
                require_non_empty("title", title)?;
                require_non_empty("body", body)?;
            }
            Action::ControlDevice {
                device_id,
                command,
                params,
            } => {
                require_non_empty("device_id", device_id)?;
                require_non_empty("command", command)?;
                if command == "set_property" {
                    let property = params
                        .get("property")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    require_non_empty("property", property)?;
                    if params.get("value").is_none() {
                        anyhow::bail!("value cannot be empty");
                    }
                }
            }
            Action::RunInference { prompt } => require_non_empty("prompt", prompt)?,
            Action::SendMessage { channel, message } => {
                require_non_empty("channel", channel)?;
                require_non_empty("message", message)?;
            }
            Action::RunScript { script_path } => require_non_empty("script_path", script_path)?,
            Action::HttpRequest { url, method, .. } => {
                require_non_empty("url", url)?;
                require_non_empty("method", method)?;
            }
            Action::ValidateScreenPlan { graph } => {
                if graph.nodes().is_empty() {
                    anyhow::bail!("screen action graph cannot be empty");
                }
                let _ = graph.topological_order()?;
            }
            Action::VehicleCommand {
                device_id, command, ..
            } => {
                require_non_empty("device_id", device_id)?;
                require_non_empty("command", command)?;
            }
        }

        Ok(())
    }
}
