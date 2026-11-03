//! 自然语言设备命令解析器
//!
//! 将中文/英文自然语言指令解析为结构化的设备意图，
//! 支持动作识别、设备引用提取和参数推断。

use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::iot::types::DeviceType;

// ─── 类型定义 ───────────────────────────────────────────

/// 设备引用方式
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeviceReference {
    /// 按名称引用 ("客厅灯")
    ByName(String),
    /// 按房间+类型引用 ("卧室的空调")
    ByRoomAndType(String, DeviceType),
    /// 按 ID 引用 ("light-001")
    ById(String),
    /// 从上下文推断
    Ambient,
}

/// 动作类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActionType {
    TurnOn,
    TurnOff,
    Toggle,
    SetProperty(String),
    GetState,
    Increase(String),
    Decrease(String),
}

/// 解析后的设备意图
#[derive(Debug, Clone)]
pub struct DeviceIntent {
    pub device_ref: DeviceReference,
    pub action: ActionType,
    pub parameters: HashMap<String, Value>,
    pub raw_text: String,
}

/// 自然语言设备命令解析器
pub struct DeviceIntentParser {
    action_on: Vec<String>,
    action_off: Vec<String>,
    action_increase: Vec<String>,
    action_decrease: Vec<String>,
    action_set: Vec<String>,
    action_query: Vec<String>,
    device_type_keywords: HashMap<String, DeviceType>,
    room_keywords: Vec<String>,
    property_keywords: HashMap<String, String>,
    modifier_values: HashMap<String, i64>,
}

// ─── 构造与关键字表 ─────────────────────────────────────

impl DeviceIntentParser {
    /// 创建解析器，初始化所有关键字表
    pub fn new() -> Self {
        let action_on = vec![
            "打开",
            "开启",
            "启动",
            "开始",
            "锁门",
            "开",
            "turn on",
            "open",
            "start",
            "switch on",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let action_off = vec![
            "关闭",
            "关掉",
            "停止",
            "关",
            "turn off",
            "close",
            "stop",
            "switch off",
            "shut",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let action_increase = vec![
            "调高", "调亮", "增大", "加大", "升高", "提高", "increase", "raise", "up", "louder",
            "brighter", "warmer",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let action_decrease = vec![
            "调低", "调暗", "减小", "降低", "减少", "decrease", "lower", "down", "quieter",
            "dimmer", "cooler",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let action_set = vec![
            "设置",
            "设为",
            "调到",
            "调整到",
            "set to",
            "set",
            "change to",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let action_query = vec![
            "查看", "查询", "状态", "多少", "什么", "how", "what", "status", "check",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let mut device_type_keywords = HashMap::new();
        for (kw, dt) in [
            ("灯", DeviceType::Light),
            ("灯光", DeviceType::Light),
            ("light", DeviceType::Light),
            ("lamp", DeviceType::Light),
            ("空调", DeviceType::Thermostat),
            ("温控", DeviceType::Thermostat),
            ("thermostat", DeviceType::Thermostat),
            ("ac", DeviceType::Thermostat),
            ("air conditioner", DeviceType::Thermostat),
            ("摄像头", DeviceType::Camera),
            ("监控", DeviceType::Camera),
            ("camera", DeviceType::Camera),
            ("门锁", DeviceType::Lock),
            ("锁", DeviceType::Lock),
            ("lock", DeviceType::Lock),
            ("音箱", DeviceType::Speaker),
            ("音响", DeviceType::Speaker),
            ("speaker", DeviceType::Speaker),
            ("电视", DeviceType::SmartTv),
            ("TV", DeviceType::SmartTv),
            ("tv", DeviceType::SmartTv),
            ("窗帘", DeviceType::Other("curtain".into())),
            ("curtain", DeviceType::Other("curtain".into())),
            ("blind", DeviceType::Other("curtain".into())),
            ("风扇", DeviceType::Other("fan".into())),
            ("fan", DeviceType::Other("fan".into())),
            ("加湿器", DeviceType::Other("humidifier".into())),
            ("humidifier", DeviceType::Other("humidifier".into())),
            ("扫地机", DeviceType::RobotVacuum),
            ("vacuum", DeviceType::RobotVacuum),
            ("robot vacuum", DeviceType::RobotVacuum),
            ("车", DeviceType::Vehicle),
            ("汽车", DeviceType::Vehicle),
            ("vehicle", DeviceType::Vehicle),
            ("car", DeviceType::Vehicle),
        ] {
            device_type_keywords.insert(kw.to_string(), dt);
        }

        let room_keywords = vec![
            "客厅",
            "卧室",
            "厨房",
            "书房",
            "浴室",
            "卫生间",
            "阳台",
            "餐厅",
            "走廊",
            "玄关",
            "车库",
            "living room",
            "bedroom",
            "kitchen",
            "study",
            "bathroom",
            "balcony",
            "dining room",
            "hallway",
            "garage",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let mut property_keywords = HashMap::new();
        for (kw, prop) in [
            ("亮度", "brightness"),
            ("brightness", "brightness"),
            ("温度", "temperature"),
            ("temperature", "temperature"),
            ("度", "temperature"),
            ("音量", "volume"),
            ("volume", "volume"),
            ("声音", "volume"),
            ("颜色", "color"),
            ("color", "color"),
            ("colour", "color"),
            ("速度", "speed"),
            ("speed", "speed"),
        ] {
            property_keywords.insert(kw.to_string(), prop.to_string());
        }

        let mut modifier_values = HashMap::new();
        for (kw, val) in [
            ("一点", 10),
            ("一些", 10),
            ("一下", 10),
            ("slightly", 10),
            ("a bit", 10),
            ("a little", 10),
            ("很多", 30),
            ("大幅", 30),
            ("a lot", 30),
            ("much", 30),
            ("一半", 50),
            ("half", 50),
        ] {
            modifier_values.insert(kw.to_string(), val);
        }

        Self {
            action_on,
            action_off,
            action_increase,
            action_decrease,
            action_set,
            action_query,
            device_type_keywords,
            room_keywords,
            property_keywords,
            modifier_values,
        }
    }
}

// ─── 解析逻辑 ───────────────────────────────────────────

impl DeviceIntentParser {
    /// 解析自然语言文本为设备意图
    pub fn parse(&self, text: &str) -> Option<DeviceIntent> {
        let lower = text.to_lowercase();

        // 1. 提取动作（最长匹配优先）
        // 如果没有明确动作但有设备引用，视为 GetState 查询
        let action = match self.extract_action(&lower) {
            Some(a) => a,
            None => {
                // 检查是否至少有设备引用，若有则默认 GetState
                let has_device = self
                    .device_type_keywords
                    .keys()
                    .any(|k| lower.contains(k.to_lowercase().as_str()));
                let has_room = self
                    .room_keywords
                    .iter()
                    .any(|r| lower.contains(r.to_lowercase().as_str()));
                if has_device || has_room {
                    ActionType::GetState
                } else {
                    return None;
                }
            }
        };

        // 2. 提取设备引用
        let device_ref = self.extract_device_ref(text, &lower);

        // 3. 提取参数
        let mut parameters = HashMap::new();

        // 特殊处理：最亮/最暗
        if lower.contains("最亮") || lower.contains("最大") {
            let prop = self.infer_property_from_text(&lower, &device_ref);
            return Some(DeviceIntent {
                device_ref,
                action: ActionType::SetProperty(prop),
                parameters: {
                    parameters.insert("value".to_string(), Value::from(100));
                    parameters
                },
                raw_text: text.to_string(),
            });
        }
        if lower.contains("最暗") || lower.contains("最小") {
            let prop = self.infer_property_from_text(&lower, &device_ref);
            return Some(DeviceIntent {
                device_ref,
                action: ActionType::SetProperty(prop),
                parameters: {
                    parameters.insert("value".to_string(), Value::from(1));
                    parameters
                },
                raw_text: text.to_string(),
            });
        }

        // 颜色提取
        if let Some(color) = self.extract_color(&lower) {
            parameters.insert("color".to_string(), Value::from(color));
        }

        // 数值提取
        if let Some(num) = self.extract_numeric_value(&lower) {
            parameters.insert("value".to_string(), Value::from(num));
        }

        // 修饰词提取
        if let Some(delta) = self.extract_modifier(&lower) {
            parameters.insert("delta".to_string(), Value::from(delta));
        }
        // 4. 完善动作：为 SetProperty/Increase/Decrease 推断属性
        let action = match action {
            ActionType::SetProperty(ref prop) if prop.is_empty() => {
                let inferred = self.infer_property_from_text(&lower, &device_ref);
                if parameters.contains_key("color") {
                    ActionType::SetProperty("color".to_string())
                } else {
                    ActionType::SetProperty(inferred)
                }
            }
            ActionType::Increase(ref prop) if prop.is_empty() => {
                let inferred = self.infer_property_from_text(&lower, &device_ref);
                ActionType::Increase(inferred)
            }
            ActionType::Decrease(ref prop) if prop.is_empty() => {
                let inferred = self.infer_property_from_text(&lower, &device_ref);
                ActionType::Decrease(inferred)
            }
            other => other,
        };

        Some(DeviceIntent {
            device_ref,
            action,
            parameters,
            raw_text: text.to_string(),
        })
    }

    /// 提取动作类型（最长关键字优先匹配）
    fn extract_action(&self, text: &str) -> Option<ActionType> {
        // 检查 set 类动作（含属性关键字 + 数值时优先）
        let has_property = self
            .property_keywords
            .keys()
            .any(|k| text.contains(k.as_str()));
        let has_number = self.extract_numeric_value(text).is_some();

        if has_property && has_number {
            if let Some(prop) = self.find_property_in_text(text) {
                return Some(ActionType::SetProperty(prop));
            }
        }

        // 按长度降序排列所有关键字，确保最长匹配
        let mut candidates: Vec<(&str, ActionType)> = Vec::new();

        for kw in &self.action_set {
            candidates.push((kw.as_str(), ActionType::SetProperty(String::new())));
        }
        for kw in &self.action_increase {
            candidates.push((kw.as_str(), ActionType::Increase(String::new())));
        }
        for kw in &self.action_decrease {
            candidates.push((kw.as_str(), ActionType::Decrease(String::new())));
        }
        for kw in &self.action_off {
            candidates.push((kw.as_str(), ActionType::TurnOff));
        }
        for kw in &self.action_on {
            candidates.push((kw.as_str(), ActionType::TurnOn));
        }
        for kw in &self.action_query {
            candidates.push((kw.as_str(), ActionType::GetState));
        }

        // 最长匹配优先
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0.len()));

        for (kw, action) in &candidates {
            if text.contains(kw) {
                return Some(action.clone());
            }
        }
        None
    }
    /// 提取设备引用
    fn extract_device_ref(&self, text: &str, lower: &str) -> DeviceReference {
        // a. 检查 "{room}的{device_type}" 模式
        if let Some((room, dt)) = self.extract_room_and_type(text, lower) {
            return DeviceReference::ByRoomAndType(room, dt);
        }

        // b. 检查复合设备名（room+type 相邻，如 "客厅灯"）
        if let Some(name) = self.extract_device_name(text, lower) {
            return DeviceReference::ByName(name);
        }

        // c. 检查裸设备类型关键字
        let has_device_type = self
            .device_type_keywords
            .keys()
            .any(|kw| lower.contains(kw.to_lowercase().as_str()));

        if has_device_type {
            return DeviceReference::Ambient;
        }

        // d. 无设备引用
        DeviceReference::Ambient
    }

    /// 提取 "{room}的{device_type}" 模式（仅匹配含 "的" 分隔符的情况）
    fn extract_room_and_type(&self, _text: &str, lower: &str) -> Option<(String, DeviceType)> {
        for room in &self.room_keywords {
            let room_lower = room.to_lowercase();
            if !lower.contains(room_lower.as_str()) {
                continue;
            }
            let room_end = lower.find(room_lower.as_str())? + room_lower.len();
            let after_room = &lower[room_end..];

            // 中文模式要求 "的" 分隔符；英文模式要求空格分隔
            let device_part = if after_room.starts_with('的') {
                after_room.trim_start_matches('的')
            } else if after_room.starts_with(' ') {
                after_room.trim_start()
            } else {
                // 无分隔符 → 交给 extract_device_name 处理复合名
                continue;
            };

            // 按关键字长度降序匹配设备类型
            let mut type_kws: Vec<_> = self.device_type_keywords.iter().collect();
            type_kws.sort_by_key(|keyword| std::cmp::Reverse(keyword.0.len()));

            for (kw, dt) in &type_kws {
                if device_part.starts_with(kw.to_lowercase().as_str()) {
                    return Some((room.clone(), (*dt).clone()));
                }
            }
        }
        None
    }

    /// 提取复合设备名（如 "客厅灯" = room + type 相邻）
    fn extract_device_name(&self, text: &str, lower: &str) -> Option<String> {
        for room in &self.room_keywords {
            let room_lower = room.to_lowercase();
            if let Some(pos) = lower.find(room_lower.as_str()) {
                let after = &lower[pos + room_lower.len()..];
                // 不含 "的" 分隔符时视为复合名
                if after.starts_with('的') {
                    continue;
                }
                let mut type_kws: Vec<_> = self.device_type_keywords.iter().collect();
                type_kws.sort_by_key(|keyword| std::cmp::Reverse(keyword.0.len()));
                for (kw, _dt) in &type_kws {
                    if after.starts_with(kw.to_lowercase().as_str()) {
                        // 从原始文本中提取（保留原始大小写）
                        let end = pos + room_lower.len() + kw.len();
                        let name = &text[pos..end];
                        return Some(name.to_string());
                    }
                }
            }
        }
        None
    }
    /// 提取数值
    fn extract_numeric_value(&self, text: &str) -> Option<i64> {
        let re = Regex::new(r"(\d+)").ok()?;
        let caps = re.captures(text)?;
        caps.get(1)?.as_str().parse::<i64>().ok()
    }

    /// 提取修饰词对应的 delta 值
    fn extract_modifier(&self, text: &str) -> Option<i64> {
        let mut best: Option<(usize, i64)> = None;
        for (kw, val) in &self.modifier_values {
            if text.contains(kw.as_str()) && best.as_ref().is_none_or(|b| kw.len() > b.0) {
                best = Some((kw.len(), *val));
            }
        }
        best.map(|(_, v)| v)
    }

    /// 提取颜色关键字
    fn extract_color(&self, text: &str) -> Option<String> {
        let color_map = [
            ("红色", "red"),
            ("红", "red"),
            ("red", "red"),
            ("蓝色", "blue"),
            ("蓝", "blue"),
            ("blue", "blue"),
            ("绿色", "green"),
            ("绿", "green"),
            ("green", "green"),
            ("黄色", "yellow"),
            ("黄", "yellow"),
            ("yellow", "yellow"),
            ("白色", "white"),
            ("白", "white"),
            ("white", "white"),
            ("紫色", "purple"),
            ("紫", "purple"),
            ("purple", "purple"),
            ("橙色", "orange"),
            ("橙", "orange"),
            ("orange", "orange"),
        ];
        // 最长匹配
        let mut best: Option<(usize, &str)> = None;
        for (kw, color) in &color_map {
            if text.contains(kw) && best.as_ref().is_none_or(|b| kw.len() > b.0) {
                best = Some((kw.len(), color));
            }
        }
        best.map(|(_, c)| c.to_string())
    }

    /// 从文本中查找属性关键字
    fn find_property_in_text(&self, text: &str) -> Option<String> {
        let mut best: Option<(usize, String)> = None;
        for (kw, prop) in &self.property_keywords {
            if text.contains(kw.as_str()) && best.as_ref().is_none_or(|b| kw.len() > b.0) {
                best = Some((kw.len(), prop.clone()));
            }
        }
        best.map(|(_, p)| p)
    }

    /// 根据文本上下文和设备引用推断属性名
    fn infer_property_from_text(&self, text: &str, _device_ref: &DeviceReference) -> String {
        // 先检查文本中是否有明确的属性关键字
        if let Some(prop) = self.find_property_in_text(text) {
            // "度" 单独出现时可能是温度也可能是亮度，需要结合设备上下文
            if prop != "temperature" || text.contains("温度") || text.contains("temperature") {
                return prop;
            }
        }

        // 根据设备类型关键字推断
        let light_kws = ["灯", "light", "lamp", "灯光"];
        let temp_kws = ["空调", "温控", "thermostat", "ac"];
        let vol_kws = ["音箱", "音响", "speaker", "音量", "volume", "声音"];

        for kw in &vol_kws {
            if text.contains(kw) {
                return "volume".to_string();
            }
        }
        for kw in &temp_kws {
            if text.contains(kw) {
                return "temperature".to_string();
            }
        }
        for kw in &light_kws {
            if text.contains(kw) {
                return "brightness".to_string();
            }
        }

        // 默认
        "brightness".to_string()
    }
}

impl Default for DeviceIntentParser {
    fn default() -> Self {
        Self::new()
    }
}
// ─── 测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> DeviceIntentParser {
        DeviceIntentParser::new()
    }

    #[test]
    fn parse_turn_on_living_room_light() {
        let intent = parser().parse("打开客厅灯").unwrap();
        assert_eq!(intent.action, ActionType::TurnOn);
        assert_eq!(intent.device_ref, DeviceReference::ByName("客厅灯".into()));
    }

    #[test]
    fn parse_turn_off_bedroom_ac() {
        let intent = parser().parse("关闭卧室的空调").unwrap();
        assert_eq!(intent.action, ActionType::TurnOff);
        assert_eq!(
            intent.device_ref,
            DeviceReference::ByRoomAndType("卧室".into(), DeviceType::Thermostat)
        );
    }

    #[test]
    fn parse_dim_light_slightly() {
        let intent = parser().parse("把灯调暗一点").unwrap();
        assert_eq!(intent.action, ActionType::Decrease("brightness".into()));
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
        assert_eq!(intent.parameters.get("delta"), Some(&Value::from(10)));
    }

    #[test]
    fn parse_brighten_light_a_lot() {
        let intent = parser().parse("把灯调亮很多").unwrap();
        assert_eq!(intent.action, ActionType::Increase("brightness".into()));
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
        assert_eq!(intent.parameters.get("delta"), Some(&Value::from(30)));
    }

    #[test]
    fn parse_set_temperature() {
        let intent = parser().parse("温度设到25度").unwrap();
        assert_eq!(intent.action, ActionType::SetProperty("temperature".into()));
        assert_eq!(intent.parameters.get("value"), Some(&Value::from(25)));
    }

    #[test]
    fn parse_turn_on_english() {
        let intent = parser().parse("turn on the light").unwrap();
        assert_eq!(intent.action, ActionType::TurnOn);
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }
    #[test]
    fn parse_increase_volume() {
        let intent = parser().parse("调高音量").unwrap();
        assert_eq!(intent.action, ActionType::Increase("volume".into()));
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }

    #[test]
    fn parse_decrease_temperature() {
        let intent = parser().parse("调低温度").unwrap();
        assert_eq!(intent.action, ActionType::Decrease("temperature".into()));
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }

    #[test]
    fn parse_query_state() {
        let intent = parser().parse("查看空调状态").unwrap();
        assert_eq!(intent.action, ActionType::GetState);
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }

    #[test]
    fn parse_set_brightness_value() {
        let intent = parser().parse("亮度调到80").unwrap();
        assert_eq!(intent.action, ActionType::SetProperty("brightness".into()));
        assert_eq!(intent.parameters.get("value"), Some(&Value::from(80)));
    }

    #[test]
    fn parse_room_and_type_pattern() {
        let intent = parser().parse("卧室的灯").unwrap();
        assert_eq!(
            intent.device_ref,
            DeviceReference::ByRoomAndType("卧室".into(), DeviceType::Light)
        );
    }

    #[test]
    fn parse_compound_device_name() {
        let intent = parser().parse("打开客厅灯").unwrap();
        assert_eq!(intent.device_ref, DeviceReference::ByName("客厅灯".into()));
    }

    #[test]
    fn parse_no_action_returns_none() {
        // 无动作关键字且无设备引用 → None
        let result = parser().parse("你好世界");
        assert!(result.is_none());
    }

    #[test]
    fn parse_no_device_uses_ambient() {
        let intent = parser().parse("打开").unwrap();
        assert_eq!(intent.action, ActionType::TurnOn);
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }
    #[test]
    fn parse_lock_door() {
        let intent = parser().parse("锁门").unwrap();
        assert_eq!(intent.action, ActionType::TurnOn);
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }

    #[test]
    fn parse_unlock_door() {
        let intent = parser().parse("开锁").unwrap();
        assert_eq!(intent.action, ActionType::TurnOn);
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }

    #[test]
    fn parse_set_color() {
        let intent = parser().parse("把灯设为红色").unwrap();
        assert_eq!(intent.action, ActionType::SetProperty("color".into()));
        assert_eq!(intent.parameters.get("color"), Some(&Value::from("red")));
    }

    #[test]
    fn parse_max_brightness() {
        let intent = parser().parse("灯开到最亮").unwrap();
        assert_eq!(intent.action, ActionType::SetProperty("brightness".into()));
        assert_eq!(intent.parameters.get("value"), Some(&Value::from(100)));
    }

    #[test]
    fn parse_min_brightness() {
        let intent = parser().parse("灯调到最暗").unwrap();
        assert_eq!(intent.action, ActionType::SetProperty("brightness".into()));
        assert_eq!(intent.parameters.get("value"), Some(&Value::from(1)));
    }

    #[test]
    fn parse_turn_on_tv() {
        let intent = parser().parse("打开电视").unwrap();
        assert_eq!(intent.action, ActionType::TurnOn);
        // 电视 is a device type keyword, could be ByName or Ambient
        assert!(
            intent.device_ref == DeviceReference::Ambient
                || matches!(intent.device_ref, DeviceReference::ByName(_))
        );
    }

    #[test]
    fn parse_start_vacuum() {
        let intent = parser().parse("开始扫地").unwrap();
        assert_eq!(intent.action, ActionType::TurnOn);
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }
    #[test]
    fn parse_increase_default_property() {
        let intent = parser().parse("调高一点").unwrap();
        assert_eq!(intent.action, ActionType::Increase("brightness".into()));
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }

    #[test]
    fn parse_with_percentage() {
        let intent = parser().parse("亮度调到50%").unwrap();
        assert_eq!(intent.action, ActionType::SetProperty("brightness".into()));
        assert_eq!(intent.parameters.get("value"), Some(&Value::from(50)));
    }

    #[test]
    fn parse_close_curtain() {
        let intent = parser().parse("关窗帘").unwrap();
        assert_eq!(intent.action, ActionType::TurnOff);
        assert_eq!(intent.device_ref, DeviceReference::Ambient);
    }

    #[test]
    fn parse_mixed_chinese_english() {
        let intent = parser().parse("turn off 客厅灯").unwrap();
        assert_eq!(intent.action, ActionType::TurnOff);
        assert_eq!(intent.device_ref, DeviceReference::ByName("客厅灯".into()));
    }
}
