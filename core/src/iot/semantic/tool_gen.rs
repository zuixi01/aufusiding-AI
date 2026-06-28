//! LLM 工具模式生成器
//!
//! 从设备语义档案动态创建 AgentTool 实现，桥接 LLM 函数调用到设备控制。

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};
use crate::iot::control::GenericController;
use crate::iot::types::Capability;
use super::profile::{
    CapabilityAffordance, DeviceSemanticProfile, ParamType, SemanticProfileStore,
};

// ─── DeviceControlTool ──────────────────────────────────────

/// 设备控制工具 — 实现 AgentTool trait，桥接 LLM 函数调用到设备控制
pub struct DeviceControlTool {
    tool_name: String,
    profile: DeviceSemanticProfile,
    controller: Arc<GenericController>,
}

impl DeviceControlTool {
    pub fn new(profile: DeviceSemanticProfile, controller: Arc<GenericController>) -> Self {
        let tool_name = format!("control_{}", profile.device_id.replace('-', "_"));
        Self {
            tool_name,
            profile,
            controller,
        }
    }

    /// 将 DeviceType 转为 controller 路由用的字符串
    fn device_type_str(&self) -> String {
        format!("{}", self.profile.device_type)
    }
}

#[async_trait]
impl AgentTool for DeviceControlTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn descriptor(&self) -> ToolDescriptor {
        DeviceToolGenerator::build_descriptor(&self.profile)
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let action = match params.get("action").and_then(|v| v.as_str()) {
            Some(a) => a.to_string(),
            None => return ToolResult::err("missing required parameter: action"),
        };

        let device_id = &self.profile.device_id;
        let dt = self.device_type_str();

        let result = match action.as_str() {
            "turn_on" => {
                self.controller
                    .execute(device_id, "turn_on", json!({"device_type": dt}))
                    .await
            }
            "turn_off" => {
                self.controller
                    .execute(device_id, "turn_off", json!({"device_type": dt}))
                    .await
            }
            "set_brightness" => {
                let value = params.get("brightness").cloned().unwrap_or(Value::Null);
                self.controller
                    .execute(
                        device_id,
                        "set_property",
                        json!({"property": "brightness", "value": value, "device_type": dt}),
                    )
                    .await
            }
            "set_temperature" => {
                let value = params.get("temperature").cloned().unwrap_or(Value::Null);
                self.controller
                    .execute(
                        device_id,
                        "set_property",
                        json!({"property": "temperature", "value": value, "device_type": dt}),
                    )
                    .await
            }
            "set_volume" => {
                let value = params.get("volume").cloned().unwrap_or(Value::Null);
                self.controller
                    .execute(
                        device_id,
                        "set_property",
                        json!({"property": "volume", "value": value, "device_type": dt}),
                    )
                    .await
            }
            "set_color" => {
                let value = params.get("color").cloned().unwrap_or(Value::Null);
                self.controller
                    .execute(
                        device_id,
                        "set_property",
                        json!({"property": "color", "value": value, "device_type": dt}),
                    )
                    .await
            }
            "lock" | "unlock" => {
                self.controller
                    .execute(
                        device_id,
                        "set_property",
                        json!({"property": "locked", "value": action == "lock", "device_type": dt}),
                    )
                    .await
            }
            "play" | "pause" | "stop" => {
                self.controller
                    .execute(
                        device_id,
                        "set_property",
                        json!({"property": "playback", "value": action, "device_type": dt}),
                    )
                    .await
            }
            "start_clean" | "stop_clean" => {
                self.controller
                    .execute(
                        device_id,
                        "set_property",
                        json!({"property": "cleaning", "value": action == "start_clean", "device_type": dt}),
                    )
                    .await
            }
            "get_state" => {
                self.controller
                    .execute(device_id, "get_state", json!({"device_type": dt}))
                    .await
            }
            other => return ToolResult::err(format!("unknown action: {}", other)),
        };

        match result {
            Ok(value) => ToolResult::ok(value),
            Err(e) => ToolResult::err(e.to_string()),
        }
    }
}

// ─── DeviceToolGenerator ────────────────────────────────────

/// 设备工具生成器
pub struct DeviceToolGenerator;

impl DeviceToolGenerator {
    /// 为指定房间（或全部）的设备生成工具列表
    pub fn generate_tools(
        store: &SemanticProfileStore,
        room: Option<&str>,
        controller: Arc<GenericController>,
    ) -> Vec<Arc<dyn AgentTool>> {
        let profiles: Vec<DeviceSemanticProfile> = match room {
            Some(r) => store.query_by_room(r).into_iter().cloned().collect(),
            None => store.list_all().into_iter().cloned().collect(),
        };
        profiles
            .into_iter()
            .map(|p| Arc::new(DeviceControlTool::new(p, controller.clone())) as Arc<dyn AgentTool>)
            .collect()
    }

    /// 生成单个设备的 ToolDescriptor
    pub fn build_descriptor(profile: &DeviceSemanticProfile) -> ToolDescriptor {
        let name = format!("control_{}", profile.device_id.replace('-', "_"));
        let description = format!("{} — {}", profile.friendly_name, profile.purpose);
        let parameters_schema = Self::build_parameters_schema(&profile.affordances);
        ToolDescriptor {
            name,
            description,
            parameters_schema,
        }
    }

    /// 从 affordances 构建 JSON Schema
    fn build_parameters_schema(affordances: &[CapabilityAffordance]) -> Value {
        let mut actions: Vec<String> = Vec::new();
        let mut has_any = false;

        for aff in affordances {
            has_any = true;
            match &aff.capability {
                Capability::Switch => {
                    push_unique(&mut actions, "turn_on");
                    push_unique(&mut actions, "turn_off");
                }
                Capability::Dimmer => push_unique(&mut actions, "set_brightness"),
                Capability::ColorControl => push_unique(&mut actions, "set_color"),
                Capability::TemperatureSet => push_unique(&mut actions, "set_temperature"),
                Capability::VolumeControl => push_unique(&mut actions, "set_volume"),
                Capability::LockUnlock => {
                    push_unique(&mut actions, "lock");
                    push_unique(&mut actions, "unlock");
                }
                Capability::MediaPlayback => {
                    push_unique(&mut actions, "play");
                    push_unique(&mut actions, "pause");
                    push_unique(&mut actions, "stop");
                }
                Capability::StartClean => {
                    push_unique(&mut actions, "start_clean");
                    push_unique(&mut actions, "stop_clean");
                }
                _ => {}
            }
        }

        if has_any {
            push_unique(&mut actions, "get_state");
        }

        // Build properties map
        let mut properties = serde_json::Map::new();
        properties.insert(
            "action".into(),
            json!({
                "type": "string",
                "enum": actions,
                "description": "要执行的操作"
            }),
        );

        // Collect unique parameters across all affordances
        let mut seen_params = HashSet::new();
        for aff in affordances {
            for param in &aff.parameters {
                if seen_params.insert(param.name.clone()) {
                    properties.insert(param.name.clone(), param_type_to_schema(&param.param_type));
                }
            }
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": ["action"]
        })
    }
}

/// 将 ParamType 转为 JSON Schema 片段
fn param_type_to_schema(pt: &ParamType) -> Value {
    match pt {
        ParamType::Bool => json!({"type": "boolean"}),
        ParamType::Int { min, max } => json!({
            "type": "integer",
            "minimum": min,
            "maximum": max
        }),
        ParamType::Float { min, max } => json!({
            "type": "number",
            "minimum": min,
            "maximum": max
        }),
        ParamType::String => json!({"type": "string"}),
        ParamType::Enum(values) => json!({
            "type": "string",
            "enum": values
        }),
    }
}

/// 向 Vec 中添加不重复的字符串
fn push_unique(vec: &mut Vec<String>, val: &str) {
    if !vec.contains(&val.to_string()) {
        vec.push(val.to_string());
    }
}

// ─── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::control::MockDeviceController;
    use crate::iot::types::DeviceType;
    use super::super::profile::build_default_profile;

    fn make_light_profile() -> DeviceSemanticProfile {
        build_default_profile("light-001", "客厅照明", DeviceType::Light, Some("客厅"))
    }

    fn make_thermostat_profile() -> DeviceSemanticProfile {
        build_default_profile("thermo-001", "卧室温控", DeviceType::Thermostat, Some("卧室"))
    }

    fn make_speaker_profile() -> DeviceSemanticProfile {
        build_default_profile("speaker-001", "客厅音箱", DeviceType::Speaker, Some("客厅"))
    }

    fn setup_controller(device_id: &str, device_type: &str) -> Arc<GenericController> {
        let mut gc = GenericController::new();
        let mock = MockDeviceController::new(device_type);
        mock.register_device(device_id, device_type);
        gc.register_adapter(Box::new(mock));
        Arc::new(gc)
    }

    fn setup_store() -> SemanticProfileStore {
        let mut store = SemanticProfileStore::new();
        store.add(make_light_profile());
        store.add(make_thermostat_profile());
        store.add(make_speaker_profile());
        store
    }

    // 1. build_descriptor_light
    #[test]
    fn build_descriptor_light() {
        let profile = make_light_profile();
        let desc = DeviceToolGenerator::build_descriptor(&profile);
        assert_eq!(desc.name, "control_light_001");
        assert!(desc.description.contains("照明"));
    }

    // 2. build_descriptor_thermostat
    #[test]
    fn build_descriptor_thermostat() {
        let profile = make_thermostat_profile();
        let desc = DeviceToolGenerator::build_descriptor(&profile);
        assert_eq!(desc.name, "control_thermo_001");
        let props = &desc.parameters_schema["properties"];
        assert!(props.get("temperature").is_some());
    }
    // 3. build_descriptor_has_action_enum
    #[test]
    fn build_descriptor_has_action_enum() {
        let profile = make_light_profile();
        let desc = DeviceToolGenerator::build_descriptor(&profile);
        let action_prop = &desc.parameters_schema["properties"]["action"];
        assert_eq!(action_prop["type"], "string");
        let enum_vals = action_prop["enum"].as_array().unwrap();
        assert!(enum_vals.iter().any(|v| v == "turn_on"));
        assert!(enum_vals.iter().any(|v| v == "turn_off"));
        assert!(enum_vals.iter().any(|v| v == "get_state"));
    }

    // 4. build_descriptor_has_brightness_param
    #[test]
    fn build_descriptor_has_brightness_param() {
        let profile = make_light_profile();
        let desc = DeviceToolGenerator::build_descriptor(&profile);
        let brightness = &desc.parameters_schema["properties"]["brightness"];
        assert_eq!(brightness["type"], "integer");
        assert_eq!(brightness["minimum"], 0);
        assert_eq!(brightness["maximum"], 100);
    }

    // 5. generate_tools_all
    #[test]
    fn generate_tools_all() {
        let store = setup_store();
        let controller = setup_controller("light-001", "Light");
        let tools = DeviceToolGenerator::generate_tools(&store, None, controller);
        assert_eq!(tools.len(), 3);
    }

    // 6. generate_tools_by_room
    #[test]
    fn generate_tools_by_room() {
        let store = setup_store();
        let controller = setup_controller("light-001", "Light");
        let tools = DeviceToolGenerator::generate_tools(&store, Some("客厅"), controller);
        assert_eq!(tools.len(), 2);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"control_light_001"));
        assert!(names.contains(&"control_speaker_001"));
    }

    // 7. generate_tools_empty_store
    #[test]
    fn generate_tools_empty_store() {
        let store = SemanticProfileStore::new();
        let controller = setup_controller("x", "Light");
        let tools = DeviceToolGenerator::generate_tools(&store, None, controller);
        assert!(tools.is_empty());
    }

    // 8. execute_turn_on
    #[tokio::test]
    async fn execute_turn_on() {
        let profile = make_light_profile();
        let controller = setup_controller("light-001", "Light");
        let tool = DeviceControlTool::new(profile, controller);
        let result = tool.execute(json!({"action": "turn_on"})).await;
        assert!(result.success);
        assert_eq!(result.output["status"], "ok");
    }

    // 9. execute_set_brightness
    #[tokio::test]
    async fn execute_set_brightness() {
        let profile = make_light_profile();
        let controller = setup_controller("light-001", "Light");
        let tool = DeviceControlTool::new(profile, controller);
        let result = tool.execute(json!({"action": "set_brightness", "brightness": 80})).await;
        assert!(result.success);
        assert_eq!(result.output["status"], "ok");
    }

    // 10. execute_invalid_action
    #[tokio::test]
    async fn execute_invalid_action() {
        let profile = make_light_profile();
        let controller = setup_controller("light-001", "Light");
        let tool = DeviceControlTool::new(profile, controller);
        let result = tool.execute(json!({"action": "explode"})).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown action"));
    }
}

