//! 设备语义档案层
//!
//! 为每个设备生成语义档案，描述其能力的环境影响、参数定义和语义标签。
//! 支持按房间、能力、名称等维度查询设备。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::iot::types::{default_capabilities, Capability, DeviceType};

/// 环境影响域
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectDomain {
    Illumination,
    Temperature,
    Humidity,
    Security,
    Media,
    Cleaning,
    AirQuality,
    Energy,
    Access,
}

/// 影响方向
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectDirection {
    Increase,
    Decrease,
    Set,
    Toggle,
}

/// 环境影响
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentEffect {
    pub domain: EffectDomain,
    pub direction: EffectDirection,
    pub magnitude: f64,
}

/// 参数类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamType {
    Bool,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    String,
    Enum(Vec<String>),
}

/// 参数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDef {
    pub name: String,
    pub param_type: ParamType,
    pub unit: Option<String>,
    pub default_value: Option<Value>,
    pub description: String,
}

/// 能力语义描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityAffordance {
    pub capability: Capability,
    pub description: String,
    pub parameters: Vec<ParameterDef>,
    pub effects: Vec<EnvironmentEffect>,
    pub reversible: bool,
}

/// 设备语义档案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSemanticProfile {
    pub device_id: String,
    pub friendly_name: String,
    pub room: Option<String>,
    pub device_type: DeviceType,
    pub purpose: String,
    pub semantic_tags: Vec<String>,
    pub affordances: Vec<CapabilityAffordance>,
}

/// 语义档案存储
pub struct SemanticProfileStore {
    profiles: HashMap<String, DeviceSemanticProfile>,
}

impl SemanticProfileStore {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
        }
    }

    pub fn add(&mut self, profile: DeviceSemanticProfile) {
        self.profiles.insert(profile.device_id.clone(), profile);
    }

    pub fn get(&self, device_id: &str) -> Option<&DeviceSemanticProfile> {
        self.profiles.get(device_id)
    }

    pub fn remove(&mut self, device_id: &str) -> Option<DeviceSemanticProfile> {
        self.profiles.remove(device_id)
    }

    pub fn list_all(&self) -> Vec<&DeviceSemanticProfile> {
        self.profiles.values().collect()
    }

    pub fn query_by_room(&self, room: &str) -> Vec<&DeviceSemanticProfile> {
        self.profiles
            .values()
            .filter(|p| p.room.as_deref() == Some(room))
            .collect()
    }

    pub fn query_by_capability(&self, cap: &Capability) -> Vec<&DeviceSemanticProfile> {
        self.profiles
            .values()
            .filter(|p| p.affordances.iter().any(|a| a.capability == *cap))
            .collect()
    }

    /// 按名称模糊搜索，返回 (档案引用, 相似度) 按分数降序
    pub fn find_by_name(&self, name: &str) -> Vec<(&DeviceSemanticProfile, f64)> {
        let mut results: Vec<(&DeviceSemanticProfile, f64)> = self
            .profiles
            .values()
            .map(|p| (p, similarity_score(name, &p.friendly_name)))
            .filter(|(_, s)| *s > 0.4)
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}

/// 名称相似度计算
///
/// - target 包含 query 子串 -> 0.9
/// - query 包含 target 子串 -> 0.8
/// - 否则：字符重叠率 = 2 * common_chars / (len_a + len_b)
/// - 使用 chars() 支持 Unicode/中文
fn similarity_score(query: &str, target: &str) -> f64 {
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let t: Vec<char> = target.to_lowercase().chars().collect();

    if q.is_empty() || t.is_empty() {
        return 0.0;
    }

    let qs: String = q.iter().collect();
    let ts: String = t.iter().collect();

    if qs == ts {
        return 1.0;
    }
    if ts.contains(&qs) {
        return 0.9;
    }
    if qs.contains(&ts) {
        return 0.8;
    }

    let mut t_remaining: Vec<char> = t.clone();
    let mut common = 0usize;
    for ch in &q {
        if let Some(pos) = t_remaining.iter().position(|c| c == ch) {
            common += 1;
            t_remaining.remove(pos);
        }
    }
    2.0 * common as f64 / (q.len() + t.len()) as f64
}

// ─── 默认档案构建 ─────────────────────────────────────

fn effect(domain: EffectDomain, dir: EffectDirection, mag: f64) -> EnvironmentEffect {
    EnvironmentEffect {
        domain,
        direction: dir,
        magnitude: mag,
    }
}

fn param_int(name: &str, min: i64, max: i64, unit: &str, desc: &str) -> ParameterDef {
    ParameterDef {
        name: name.into(),
        param_type: ParamType::Int { min, max },
        unit: Some(unit.into()),
        default_value: None,
        description: desc.into(),
    }
}

fn build_affordance(cap: &Capability) -> CapabilityAffordance {
    use Capability::*;
    use EffectDirection::*;
    use EffectDomain::*;
    match cap {
        Switch => CapabilityAffordance {
            capability: cap.clone(),
            description: "开关控制".into(),
            parameters: vec![],
            effects: vec![effect(Illumination, Toggle, 1.0)],
            reversible: true,
        },
        Dimmer => CapabilityAffordance {
            capability: cap.clone(),
            description: "亮度调节".into(),
            parameters: vec![param_int("brightness", 0, 100, "%", "亮度百分比")],
            effects: vec![effect(Illumination, Set, 1.0)],
            reversible: true,
        },
        ColorControl => CapabilityAffordance {
            capability: cap.clone(),
            description: "颜色控制".into(),
            parameters: vec![ParameterDef {
                name: "color".into(),
                param_type: ParamType::String,
                unit: None,
                default_value: None,
                description: "颜色值".into(),
            }],
            effects: vec![effect(Illumination, Set, 1.0)],
            reversible: true,
        },
        TemperatureRead => CapabilityAffordance {
            capability: cap.clone(),
            description: "温度读取".into(),
            parameters: vec![],
            effects: vec![effect(Temperature, Set, 0.0)],
            reversible: false,
        },
        TemperatureSet => CapabilityAffordance {
            capability: cap.clone(),
            description: "温度设置".into(),
            parameters: vec![param_int("temperature", 16, 30, "°C", "目标温度")],
            effects: vec![effect(Temperature, Set, 1.0)],
            reversible: true,
        },
        HumidityRead => CapabilityAffordance {
            capability: cap.clone(),
            description: "湿度读取".into(),
            parameters: vec![],
            effects: vec![effect(Humidity, Set, 0.0)],
            reversible: false,
        },
        LockUnlock => CapabilityAffordance {
            capability: cap.clone(),
            description: "门锁控制".into(),
            parameters: vec![],
            effects: vec![effect(Security, Toggle, 1.0)],
            reversible: true,
        },
        MotionDetect => CapabilityAffordance {
            capability: cap.clone(),
            description: "运动检测".into(),
            parameters: vec![],
            effects: vec![effect(Security, Set, 0.0)],
            reversible: false,
        },
        VideoStream => CapabilityAffordance {
            capability: cap.clone(),
            description: "视频流".into(),
            parameters: vec![],
            effects: vec![effect(Security, Set, 1.0)],
            reversible: true,
        },
        VideoRecord => CapabilityAffordance {
            capability: cap.clone(),
            description: "视频录制".into(),
            parameters: vec![],
            effects: vec![effect(Security, Set, 1.0)],
            reversible: true,
        },
        MediaPlayback => CapabilityAffordance {
            capability: cap.clone(),
            description: "媒体播放".into(),
            parameters: vec![],
            effects: vec![effect(Media, Set, 1.0)],
            reversible: true,
        },
        VolumeControl => CapabilityAffordance {
            capability: cap.clone(),
            description: "音量控制".into(),
            parameters: vec![param_int("volume", 0, 100, "%", "音量百分比")],
            effects: vec![effect(Media, Set, 1.0)],
            reversible: true,
        },
        ScreenMirror => CapabilityAffordance {
            capability: cap.clone(),
            description: "屏幕镜像".into(),
            parameters: vec![],
            effects: vec![effect(Media, Set, 1.0)],
            reversible: true,
        },
        Print => CapabilityAffordance {
            capability: cap.clone(),
            description: "打印".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        PrintStatus => CapabilityAffordance {
            capability: cap.clone(),
            description: "打印状态".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        ScanDocument => CapabilityAffordance {
            capability: cap.clone(),
            description: "扫描文档".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        FileShare => CapabilityAffordance {
            capability: cap.clone(),
            description: "文件共享".into(),
            parameters: vec![],
            effects: vec![],
            reversible: true,
        },
        StorageInfo => CapabilityAffordance {
            capability: cap.clone(),
            description: "存储信息".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        WakeOnLan => CapabilityAffordance {
            capability: cap.clone(),
            description: "远程唤醒".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        RemoteShell => CapabilityAffordance {
            capability: cap.clone(),
            description: "远程终端".into(),
            parameters: vec![],
            effects: vec![],
            reversible: true,
        },
        ClipboardShare => CapabilityAffordance {
            capability: cap.clone(),
            description: "剪贴板共享".into(),
            parameters: vec![],
            effects: vec![],
            reversible: true,
        },
        FileTransfer => CapabilityAffordance {
            capability: cap.clone(),
            description: "文件传输".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        VehicleLocation => CapabilityAffordance {
            capability: cap.clone(),
            description: "车辆定位".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        VehicleDiagnostics => CapabilityAffordance {
            capability: cap.clone(),
            description: "车辆诊断".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        VehicleClimate => CapabilityAffordance {
            capability: cap.clone(),
            description: "车辆空调".into(),
            parameters: vec![param_int("temperature", 16, 30, "°C", "车内温度")],
            effects: vec![effect(Temperature, Set, 1.0)],
            reversible: true,
        },
        VehicleLock => CapabilityAffordance {
            capability: cap.clone(),
            description: "车辆锁定".into(),
            parameters: vec![],
            effects: vec![effect(Security, Toggle, 1.0)],
            reversible: true,
        },
        VehicleCharge => CapabilityAffordance {
            capability: cap.clone(),
            description: "充电控制".into(),
            parameters: vec![],
            effects: vec![effect(Energy, Set, 1.0)],
            reversible: true,
        },
        HeartRate => CapabilityAffordance {
            capability: cap.clone(),
            description: "心率监测".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        StepCount => CapabilityAffordance {
            capability: cap.clone(),
            description: "步数统计".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        SleepTracking => CapabilityAffordance {
            capability: cap.clone(),
            description: "睡眠追踪".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        BloodOxygen => CapabilityAffordance {
            capability: cap.clone(),
            description: "血氧监测".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        PowerConsumption => CapabilityAffordance {
            capability: cap.clone(),
            description: "功耗监测".into(),
            parameters: vec![],
            effects: vec![effect(Energy, Set, 0.0)],
            reversible: false,
        },
        SolarProduction => CapabilityAffordance {
            capability: cap.clone(),
            description: "太阳能产出".into(),
            parameters: vec![],
            effects: vec![effect(Energy, Set, 1.0)],
            reversible: false,
        },
        BatteryLevel => CapabilityAffordance {
            capability: cap.clone(),
            description: "电池电量".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        StartClean => CapabilityAffordance {
            capability: cap.clone(),
            description: "开始清扫".into(),
            parameters: vec![],
            effects: vec![effect(Cleaning, Set, 1.0)],
            reversible: true,
        },
        CleaningMap => CapabilityAffordance {
            capability: cap.clone(),
            description: "清扫地图".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        CleaningSchedule => CapabilityAffordance {
            capability: cap.clone(),
            description: "清扫计划".into(),
            parameters: vec![],
            effects: vec![],
            reversible: true,
        },
        Print3DStart => CapabilityAffordance {
            capability: cap.clone(),
            description: "开始3D打印".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        Print3DStatus => CapabilityAffordance {
            capability: cap.clone(),
            description: "3D打印状态".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        Print3DTemperature => CapabilityAffordance {
            capability: cap.clone(),
            description: "3D打印温度".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
        DroneArm => CapabilityAffordance {
            capability: cap.clone(),
            description: "无人机解锁".into(),
            parameters: vec![],
            effects: vec![effect(Security, Toggle, 1.0)],
            reversible: true,
        },
        DroneFly => CapabilityAffordance {
            capability: cap.clone(),
            description: "无人机飞行".into(),
            parameters: vec![],
            effects: vec![],
            reversible: true,
        },
        DroneCamera => CapabilityAffordance {
            capability: cap.clone(),
            description: "无人机摄像".into(),
            parameters: vec![],
            effects: vec![],
            reversible: true,
        },
        CustomCommand(_) => CapabilityAffordance {
            capability: cap.clone(),
            description: "自定义命令".into(),
            parameters: vec![],
            effects: vec![],
            reversible: false,
        },
    }
}

fn device_purpose_and_tags(dt: &DeviceType) -> (String, Vec<String>) {
    match dt {
        DeviceType::Light => (
            "照明控制".into(),
            vec!["照明".into(), "灯光".into(), "light".into()],
        ),
        DeviceType::Thermostat => (
            "温度调节".into(),
            vec!["温控".into(), "空调".into(), "thermostat".into()],
        ),
        DeviceType::Camera => (
            "安防监控".into(),
            vec!["摄像头".into(), "监控".into(), "camera".into()],
        ),
        DeviceType::Lock => (
            "门锁安防".into(),
            vec!["门锁".into(), "安全".into(), "lock".into()],
        ),
        DeviceType::Sensor => ("环境感知".into(), vec!["传感器".into(), "sensor".into()]),
        DeviceType::Printer => ("打印服务".into(), vec!["打印机".into(), "printer".into()]),
        DeviceType::Speaker => (
            "音频播放".into(),
            vec!["音箱".into(), "音乐".into(), "speaker".into()],
        ),
        DeviceType::Nas => ("网络存储".into(), vec!["NAS".into(), "存储".into()]),
        DeviceType::Computer => ("电脑控制".into(), vec!["电脑".into(), "computer".into()]),
        DeviceType::Vehicle => (
            "车辆控制".into(),
            vec!["汽车".into(), "车辆".into(), "vehicle".into()],
        ),
        DeviceType::Wearable => (
            "健康监测".into(),
            vec!["穿戴".into(), "手表".into(), "wearable".into()],
        ),
        DeviceType::SmartTv => (
            "影音娱乐".into(),
            vec!["电视".into(), "TV".into(), "smart_tv".into()],
        ),
        DeviceType::RobotVacuum => (
            "自动清扫".into(),
            vec!["扫地机".into(), "吸尘器".into(), "vacuum".into()],
        ),
        DeviceType::EnergyMonitor => (
            "能源监测".into(),
            vec!["能耗".into(), "电量".into(), "energy".into()],
        ),
        DeviceType::Printer3D => ("3D打印".into(), vec!["3D打印机".into(), "printer3d".into()]),
        DeviceType::Drone => ("无人机".into(), vec!["无人机".into(), "drone".into()]),
        DeviceType::Intercom => (
            "门禁对讲".into(),
            vec!["对讲机".into(), "门禁".into(), "intercom".into()],
        ),
        _ => ("通用设备".into(), vec!["device".into()]),
    }
}

/// 从设备类型自动构建默认语义档案
pub fn build_default_profile(
    device_id: &str,
    name: &str,
    device_type: DeviceType,
    room: Option<&str>,
) -> DeviceSemanticProfile {
    let caps = default_capabilities(&device_type);
    let affordances: Vec<CapabilityAffordance> = caps.iter().map(build_affordance).collect();
    let (purpose, semantic_tags) = device_purpose_and_tags(&device_type);
    DeviceSemanticProfile {
        device_id: device_id.into(),
        friendly_name: name.into(),
        room: room.map(|r| r.to_string()),
        device_type,
        purpose,
        semantic_tags,
        affordances,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_default_profile_light() {
        let p = build_default_profile("l1", "客厅灯", DeviceType::Light, Some("客厅"));
        assert!(!p.affordances.is_empty());
        assert!(p.affordances.iter().any(|a| a
            .effects
            .iter()
            .any(|e| e.domain == EffectDomain::Illumination)));
    }

    #[test]
    fn build_default_profile_thermostat() {
        let p = build_default_profile("t1", "空调", DeviceType::Thermostat, None);
        assert!(p.affordances.iter().any(|a| a
            .effects
            .iter()
            .any(|e| e.domain == EffectDomain::Temperature)));
    }

    #[test]
    fn build_default_profile_lock() {
        let p = build_default_profile("k1", "门锁", DeviceType::Lock, None);
        assert!(p
            .affordances
            .iter()
            .any(|a| a.reversible && a.effects.iter().any(|e| e.domain == EffectDomain::Security)));
    }

    #[test]
    fn build_default_profile_camera() {
        let p = build_default_profile("c1", "摄像头", DeviceType::Camera, None);
        assert!(p
            .affordances
            .iter()
            .any(|a| a.effects.iter().any(|e| e.domain == EffectDomain::Security)));
    }

    #[test]
    fn build_default_profile_speaker() {
        let p = build_default_profile("s1", "音箱", DeviceType::Speaker, None);
        assert!(p
            .affordances
            .iter()
            .any(|a| a.effects.iter().any(|e| e.domain == EffectDomain::Media)));
    }

    #[test]
    fn build_default_profile_vehicle() {
        let p = build_default_profile("v1", "车", DeviceType::Vehicle, None);
        assert!(p.affordances.len() >= 3);
    }

    #[test]
    fn build_default_profile_all_types() {
        let types = [
            DeviceType::Light,
            DeviceType::Thermostat,
            DeviceType::Camera,
            DeviceType::Lock,
            DeviceType::Sensor,
            DeviceType::Printer,
            DeviceType::Speaker,
            DeviceType::Nas,
            DeviceType::Computer,
            DeviceType::Vehicle,
            DeviceType::Wearable,
            DeviceType::SmartTv,
            DeviceType::RobotVacuum,
            DeviceType::EnergyMonitor,
            DeviceType::Printer3D,
            DeviceType::Drone,
            DeviceType::Intercom,
        ];
        for dt in &types {
            let p = build_default_profile("x", "x", dt.clone(), None);
            assert!(
                !p.affordances.is_empty(),
                "{:?} should have affordances",
                dt
            );
        }
    }

    #[test]
    fn build_default_profile_purpose_and_tags() {
        let p = build_default_profile("l1", "灯", DeviceType::Light, None);
        assert_eq!(p.purpose, "照明控制");
        assert!(p.semantic_tags.contains(&"light".to_string()));
    }

    #[test]
    fn store_add_and_get() {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile("l1", "灯", DeviceType::Light, None));
        assert!(store.get("l1").is_some());
        assert!(store.get("l2").is_none());
    }

    #[test]
    fn store_remove() {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile("l1", "灯", DeviceType::Light, None));
        let removed = store.remove("l1");
        assert!(removed.is_some());
        assert!(store.get("l1").is_none());
    }

    #[test]
    fn store_list_all() {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile("l1", "灯1", DeviceType::Light, None));
        store.add(build_default_profile("l2", "灯2", DeviceType::Light, None));
        assert_eq!(store.list_all().len(), 2);
    }

    #[test]
    fn store_query_by_room() {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile(
            "l1",
            "客厅灯",
            DeviceType::Light,
            Some("客厅"),
        ));
        store.add(build_default_profile(
            "l2",
            "卧室灯",
            DeviceType::Light,
            Some("卧室"),
        ));
        assert_eq!(store.query_by_room("客厅").len(), 1);
        assert_eq!(store.query_by_room("客厅")[0].device_id, "l1");
    }

    #[test]
    fn store_query_by_room_empty() {
        let store = SemanticProfileStore::new();
        assert!(store.query_by_room("客厅").is_empty());
    }

    #[test]
    fn store_query_by_capability() {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile("l1", "灯", DeviceType::Light, None));
        store.add(build_default_profile(
            "t1",
            "空调",
            DeviceType::Thermostat,
            None,
        ));
        let dimmers = store.query_by_capability(&Capability::Dimmer);
        assert_eq!(dimmers.len(), 1);
        assert_eq!(dimmers[0].device_id, "l1");
    }

    #[test]
    fn store_find_by_name_exact() {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile(
            "l1",
            "客厅灯",
            DeviceType::Light,
            Some("客厅"),
        ));
        let results = store.find_by_name("客厅灯");
        assert!(!results.is_empty());
        assert_eq!(results[0].0.device_id, "l1");
        assert!((results[0].1 - 1.0).abs() < 0.01);
    }

    #[test]
    fn store_find_by_name_substring() {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile(
            "l1",
            "客厅灯",
            DeviceType::Light,
            Some("客厅"),
        ));
        let results = store.find_by_name("客厅");
        assert!(!results.is_empty());
        assert!(results[0].1 > 0.5);
    }

    #[test]
    fn store_find_by_name_no_match() {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile(
            "l1",
            "客厅灯",
            DeviceType::Light,
            None,
        ));
        let results = store.find_by_name("xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn profile_serde_roundtrip() {
        let p = build_default_profile("l1", "灯", DeviceType::Light, Some("客厅"));
        let json = serde_json::to_string(&p).unwrap();
        let back: DeviceSemanticProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.device_id, "l1");
        assert_eq!(back.affordances.len(), p.affordances.len());
    }

    #[test]
    fn affordance_serde_roundtrip() {
        let a = build_affordance(&Capability::Dimmer);
        let json = serde_json::to_string(&a).unwrap();
        let back: CapabilityAffordance = serde_json::from_str(&json).unwrap();
        assert_eq!(back.description, "亮度调节");
    }

    #[test]
    fn similarity_score_tests() {
        // 完全匹配
        assert!((similarity_score("客厅灯", "客厅灯") - 1.0).abs() < 0.01);
        // target 包含 query
        assert!((similarity_score("客厅", "客厅灯") - 0.9).abs() < 0.01);
        // query 包含 target
        assert!((similarity_score("客厅大灯", "客厅") - 0.8).abs() < 0.01);
        // 无重叠
        assert!(similarity_score("xyz", "客厅灯") < 0.01);
        // 空字符串
        assert!(similarity_score("", "") < 0.01);
        // 部分字符重叠
        let score = similarity_score("abc", "axy");
        assert!(score > 0.0 && score < 0.8);
    }
}
