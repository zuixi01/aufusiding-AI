//! 命令映射器 — 将语义动作映射为可执行的 SceneAction
//!
//! 接收解析后的语义动作（ActionType）、设备信息和能力约束，
//! 生成可直接执行的 SceneAction 列表。

use std::collections::HashMap;

use serde_json::Value;

use crate::iot::scenes::SceneAction;
use crate::iot::types::DeviceType;

use super::intent_parser::ActionType;
use super::profile::{CapabilityAffordance, ParamType};

// ─── 命令映射器 ─────────────────────────────────────────

/// 将语义动作映射为可执行的 SceneAction
pub struct DeviceCommandMapper;

impl DeviceCommandMapper {
    /// 将语义动作映射为 SceneAction 列表
    pub fn map(
        device_id: &str,
        device_type: &DeviceType,
        action: &ActionType,
        params: &HashMap<String, Value>,
        affordances: &[CapabilityAffordance],
        current_state: Option<&Value>,
    ) -> anyhow::Result<Vec<SceneAction>> {
        let dt_str = device_type_to_str(device_type);

        let actions = match action {
            ActionType::TurnOn => vec![SceneAction {
                device_id: device_id.to_string(),
                command: "turn_on".to_string(),
                params: serde_json::json!({"device_type": dt_str}),
                delay_ms: 0,
            }],

            ActionType::TurnOff => vec![SceneAction {
                device_id: device_id.to_string(),
                command: "turn_off".to_string(),
                params: serde_json::json!({"device_type": dt_str}),
                delay_ms: 0,
            }],

            ActionType::Toggle => {
                let powered = current_state
                    .and_then(|s| s.get("powered"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let cmd = if powered { "turn_off" } else { "turn_on" };
                vec![SceneAction {
                    device_id: device_id.to_string(),
                    command: cmd.to_string(),
                    params: serde_json::json!({"device_type": dt_str}),
                    delay_ms: 0,
                }]
            }

            ActionType::SetProperty(property) => {
                let value = params
                    .get("value")
                    .cloned()
                    .unwrap_or(serde_json::json!(0));

                let clamped = clamp_to_range(&value, affordances, property);
                vec![SceneAction {
                    device_id: device_id.to_string(),
                    command: "set_property".to_string(),
                    params: serde_json::json!({
                        "device_type": dt_str,
                        "property": property,
                        "value": clamped,
                    }),
                    delay_ms: 0,
                }]
            }

            ActionType::Increase(property) => {
                let delta = params
                    .get("delta")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(10);
                let current = get_current_value(current_state, property);
                let (_min, max) = find_param_range(affordances, property);
                let new_value = std::cmp::min(current + delta, max);

                vec![SceneAction {
                    device_id: device_id.to_string(),
                    command: "set_property".to_string(),
                    params: serde_json::json!({
                        "device_type": dt_str,
                        "property": property,
                        "value": new_value,
                    }),
                    delay_ms: 0,
                }]
            }
            ActionType::Decrease(property) => {
                let delta = params
                    .get("delta")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(10);
                let current = get_current_value(current_state, property);
                let (min, _max) = find_param_range(affordances, property);
                let floor = if property == "brightness" {
                    std::cmp::max(min, 1)
                } else {
                    min
                };
                let new_value = std::cmp::max(current - delta, floor);

                vec![SceneAction {
                    device_id: device_id.to_string(),
                    command: "set_property".to_string(),
                    params: serde_json::json!({
                        "device_type": dt_str,
                        "property": property,
                        "value": new_value,
                    }),
                    delay_ms: 0,
                }]
            }

            ActionType::GetState => vec![SceneAction {
                device_id: device_id.to_string(),
                command: "get_state".to_string(),
                params: serde_json::json!({"device_type": dt_str}),
                delay_ms: 0,
            }],
        };

        Ok(actions)
    }

    /// 生成人类可读的执行结果描述
    pub fn describe_action(
        device_name: &str,
        action: &ActionType,
        params: &HashMap<String, Value>,
    ) -> String {
        match action {
            ActionType::TurnOn => format!("已打开{}", device_name),
            ActionType::TurnOff => format!("已关闭{}", device_name),
            ActionType::Toggle => format!("已切换{}", device_name),
            ActionType::SetProperty(prop) => {
                let value = params.get("value").and_then(|v| v.as_i64());
                match (prop.as_str(), value) {
                    ("brightness", Some(v)) => {
                        format!("已将{}亮度调到{}%", device_name, v)
                    }
                    ("temperature", Some(v)) => {
                        format!("已将{}温度设为{}°C", device_name, v)
                    }
                    ("volume", Some(v)) => {
                        format!("已将{}音量调到{}%", device_name, v)
                    }
                    (p, Some(v)) => {
                        format!("已将{}的{}设为{}", device_name, p, v)
                    }
                    (p, None) => {
                        format!("已设置{}的{}", device_name, p)
                    }
                }
            }
            ActionType::Increase(prop) => {
                let label = property_label(prop);
                format!("已调高{}{}", device_name, label)
            }
            ActionType::Decrease(prop) => {
                let label = property_label(prop);
                format!("已调低{}{}", device_name, label)
            }
            ActionType::GetState => format!("正在查询{}状态", device_name),
        }
    }
}

// ─── 辅助函数 ───────────────────────────────────────────

/// 将 DeviceType 转为字符串，用于 GenericController 路由
fn device_type_to_str(device_type: &DeviceType) -> String {
    match device_type {
        DeviceType::Light => "light".to_string(),
        DeviceType::Thermostat => "thermostat".to_string(),
        DeviceType::Camera => "camera".to_string(),
        DeviceType::Lock => "lock".to_string(),
        DeviceType::Sensor => "sensor".to_string(),
        DeviceType::Printer => "printer".to_string(),
        DeviceType::Speaker => "speaker".to_string(),
        DeviceType::Nas => "nas".to_string(),
        DeviceType::Computer => "computer".to_string(),
        DeviceType::Vehicle => "vehicle".to_string(),
        DeviceType::Wearable => "wearable".to_string(),
        DeviceType::SmartTv => "smart_tv".to_string(),
        DeviceType::RobotVacuum => "robot_vacuum".to_string(),
        DeviceType::EnergyMonitor => "energy_monitor".to_string(),
        DeviceType::Printer3D => "printer_3d".to_string(),
        DeviceType::Drone => "drone".to_string(),
        DeviceType::Intercom => "intercom".to_string(),
        DeviceType::Other(s) => s.clone(),
    }
}

/// 从 affordances 中查找属性的参数范围 (min, max)
fn find_param_range(affordances: &[CapabilityAffordance], property: &str) -> (i64, i64) {
    for affordance in affordances {
        for param in &affordance.parameters {
            if param.name == property {
                if let ParamType::Int { min, max } = &param.param_type {
                    return (*min, *max);
                }
            }
        }
    }
    (0, 100)
}

/// 从当前状态中提取属性值
fn get_current_value(current_state: Option<&Value>, property: &str) -> i64 {
    current_state
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.get(property))
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
}

/// 将值钳制到 affordances 定义的有效范围内
fn clamp_to_range(value: &Value, affordances: &[CapabilityAffordance], property: &str) -> Value {
    if let Some(v) = value.as_i64() {
        let (min, max) = find_param_range(affordances, property);
        let clamped = v.max(min).min(max);
        serde_json::json!(clamped)
    } else {
        value.clone()
    }
}

/// 属性名到中文标签的映射
fn property_label(property: &str) -> &str {
    match property {
        "brightness" => "亮度",
        "temperature" => "温度",
        "volume" => "音量",
        "humidity" => "湿度",
        "speed" => "速度",
        _ => property,
    }
}

// ─── 测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::types::Capability;
    use super::super::profile::ParameterDef;

    fn empty_affordances() -> Vec<CapabilityAffordance> {
        vec![]
    }

    fn brightness_affordance() -> Vec<CapabilityAffordance> {
        vec![CapabilityAffordance {
            capability: Capability::Dimmer,
            description: "亮度控制".to_string(),
            parameters: vec![ParameterDef {
                name: "brightness".to_string(),
                param_type: ParamType::Int { min: 0, max: 100 },
                unit: Some("%".to_string()),
                default_value: None,
                description: "亮度百分比".to_string(),
            }],
            effects: vec![],
            reversible: true,
        }]
    }

    fn make_state(powered: bool, props: Vec<(&str, i64)>) -> Value {
        let mut properties = serde_json::Map::new();
        for (k, v) in props {
            properties.insert(k.to_string(), serde_json::json!(v));
        }
        serde_json::json!({
            "powered": powered,
            "properties": properties,
        })
    }

    #[test]
    fn map_turn_on() {
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light, &ActionType::TurnOn,
            &HashMap::new(), &empty_affordances(), None,
        ).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].command, "turn_on");
        assert_eq!(result[0].device_id, "light-001");
    }

    #[test]
    fn map_turn_off() {
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light, &ActionType::TurnOff,
            &HashMap::new(), &empty_affordances(), None,
        ).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].command, "turn_off");
    }

    #[test]
    fn map_toggle_on_to_off() {
        let state = make_state(true, vec![]);
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light, &ActionType::Toggle,
            &HashMap::new(), &empty_affordances(), Some(&state),
        ).unwrap();
        assert_eq!(result[0].command, "turn_off");
    }

    #[test]
    fn map_toggle_off_to_on() {
        let state = make_state(false, vec![]);
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light, &ActionType::Toggle,
            &HashMap::new(), &empty_affordances(), Some(&state),
        ).unwrap();
        assert_eq!(result[0].command, "turn_on");
    }

    #[test]
    fn map_toggle_no_state() {
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light, &ActionType::Toggle,
            &HashMap::new(), &empty_affordances(), None,
        ).unwrap();
        assert_eq!(result[0].command, "turn_on");
    }

    #[test]
    fn map_set_brightness() {
        let mut params = HashMap::new();
        params.insert("value".to_string(), serde_json::json!(80));
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light,
            &ActionType::SetProperty("brightness".to_string()),
            &params, &brightness_affordance(), None,
        ).unwrap();
        assert_eq!(result[0].command, "set_property");
        assert_eq!(result[0].params["value"], 80);
        assert_eq!(result[0].params["property"], "brightness");
    }

    #[test]
    fn map_increase_brightness() {
        let mut params = HashMap::new();
        params.insert("delta".to_string(), serde_json::json!(10));
        let state = make_state(true, vec![("brightness", 70)]);
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light,
            &ActionType::Increase("brightness".to_string()),
            &params, &brightness_affordance(), Some(&state),
        ).unwrap();
        assert_eq!(result[0].command, "set_property");
        assert_eq!(result[0].params["value"], 80);
    }

    #[test]
    fn map_increase_clamps_to_max() {
        let mut params = HashMap::new();
        params.insert("delta".to_string(), serde_json::json!(20));
        let state = make_state(true, vec![("brightness", 90)]);
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light,
            &ActionType::Increase("brightness".to_string()),
            &params, &brightness_affordance(), Some(&state),
        ).unwrap();
        assert_eq!(result[0].params["value"], 100);
    }

    #[test]
    fn map_decrease_brightness() {
        let mut params = HashMap::new();
        params.insert("delta".to_string(), serde_json::json!(10));
        let state = make_state(true, vec![("brightness", 50)]);
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light,
            &ActionType::Decrease("brightness".to_string()),
            &params, &brightness_affordance(), Some(&state),
        ).unwrap();
        assert_eq!(result[0].params["value"], 40);
    }

    #[test]
    fn map_decrease_clamps_to_min() {
        let mut params = HashMap::new();
        params.insert("delta".to_string(), serde_json::json!(20));
        let state = make_state(true, vec![("brightness", 5)]);
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light,
            &ActionType::Decrease("brightness".to_string()),
            &params, &brightness_affordance(), Some(&state),
        ).unwrap();
        // brightness floor is 1, not 0
        assert_eq!(result[0].params["value"], 1);
    }

    #[test]
    fn map_increase_no_state_defaults() {
        let params = HashMap::new();
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light,
            &ActionType::Increase("brightness".to_string()),
            &params, &brightness_affordance(), None,
        ).unwrap();
        // default current=50, default delta=10 → 60
        assert_eq!(result[0].params["value"], 60);
    }

    #[test]
    fn map_get_state() {
        let result = DeviceCommandMapper::map(
            "light-001", &DeviceType::Light, &ActionType::GetState,
            &HashMap::new(), &empty_affordances(), None,
        ).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].command, "get_state");
    }

    #[test]
    fn describe_action_turn_on() {
        let desc = DeviceCommandMapper::describe_action(
            "客厅灯", &ActionType::TurnOn, &HashMap::new(),
        );
        assert_eq!(desc, "已打开客厅灯");
    }

    #[test]
    fn describe_action_set_brightness() {
        let mut params = HashMap::new();
        params.insert("value".to_string(), serde_json::json!(80));
        let desc = DeviceCommandMapper::describe_action(
            "客厅灯",
            &ActionType::SetProperty("brightness".to_string()),
            &params,
        );
        assert_eq!(desc, "已将客厅灯亮度调到80%");
    }

    #[test]
    fn describe_action_increase() {
        let desc = DeviceCommandMapper::describe_action(
            "客厅灯",
            &ActionType::Increase("brightness".to_string()),
            &HashMap::new(),
        );
        assert_eq!(desc, "已调高客厅灯亮度");
    }
}