//! IoT 类型系统 — 设备类型、协议、能力的类型安全枚举
//!
//! 替代原有 String 类型字段，通过 `From<&str>` 保持向后兼容。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

// ─── 设备类型 ───────────────────────────────────────────

/// 设备类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DeviceType {
    // 智能家居
    Light,
    Thermostat,
    Camera,
    Lock,
    Sensor,
    // LAN 设备
    Printer,
    Speaker,
    Nas,
    Computer,
    // 车辆
    Vehicle,
    // 高级设备
    Wearable,
    SmartTv,
    RobotVacuum,
    EnergyMonitor,
    Printer3D,
    Drone,
    /// 门禁对讲
    Intercom,
    /// 兜底
    Other(String),
}

impl From<&str> for DeviceType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "light" => Self::Light,
            "thermostat" => Self::Thermostat,
            "camera" => Self::Camera,
            "lock" => Self::Lock,
            "sensor" => Self::Sensor,
            "printer" => Self::Printer,
            "speaker" => Self::Speaker,
            "nas" => Self::Nas,
            "computer" => Self::Computer,
            "vehicle" => Self::Vehicle,
            "wearable" => Self::Wearable,
            "smart_tv" | "smarttv" => Self::SmartTv,
            "robot_vacuum" | "robotvacuum" => Self::RobotVacuum,
            "energy_monitor" | "energymonitor" => Self::EnergyMonitor,
            "3d_printer" | "printer3d" => Self::Printer3D,
            "drone" => Self::Drone,
            "intercom" => Self::Intercom,
            other => Self::Other(other.to_string()),
        }
    }
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(s) => write!(f, "{}", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

// ─── 通信协议 ───────────────────────────────────────────

/// 通信协议枚举
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Protocol {
    // 智能家居
    Matter,
    HomeKit,
    Zigbee,
    ZWave,
    // 通用
    Mqtt,
    Http,
    Https,
    // 发现/媒体
    Mdns,
    Ssdp,
    Upnp,
    Dlna,
    // 打印
    Ipp,
    // 存储
    Smb,
    Nfs,
    WebDav,
    // 蓝牙
    Ble,
    // 摄像头
    Onvif,
    Rtsp,
    // 车辆
    Obd2,
    TeslaApi,
    SmartCarApi,
    /// 兜底
    Other(String),
}

impl From<&str> for Protocol {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "matter" => Self::Matter,
            "homekit" => Self::HomeKit,
            "zigbee" => Self::Zigbee,
            "zwave" | "z-wave" => Self::ZWave,
            "mqtt" => Self::Mqtt,
            "http" => Self::Http,
            "https" => Self::Https,
            "mdns" => Self::Mdns,
            "ssdp" => Self::Ssdp,
            "upnp" => Self::Upnp,
            "dlna" => Self::Dlna,
            "ipp" => Self::Ipp,
            "smb" => Self::Smb,
            "nfs" => Self::Nfs,
            "webdav" => Self::WebDav,
            "ble" | "bluetooth" => Self::Ble,
            "onvif" => Self::Onvif,
            "rtsp" => Self::Rtsp,
            "obd2" | "obd-ii" => Self::Obd2,
            "tesla" | "tesla_api" => Self::TeslaApi,
            "smartcar" | "smartcar_api" => Self::SmartCarApi,
            other => Self::Other(other.to_string()),
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(s) => write!(f, "{}", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

// ─── 设备能力 ───────────────────────────────────────────
/// 设备能力（设备声明自己能做什么）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Capability {
    // 电源/灯光
    Switch,
    Dimmer,
    ColorControl,
    // 气候
    TemperatureRead,
    TemperatureSet,
    HumidityRead,
    // 安防
    LockUnlock,
    MotionDetect,
    VideoStream,
    VideoRecord,
    // 媒体
    MediaPlayback,
    VolumeControl,
    ScreenMirror,
    // 打印
    Print,
    PrintStatus,
    ScanDocument,
    // 存储
    FileShare,
    StorageInfo,
    // 电脑
    WakeOnLan,
    RemoteShell,
    ClipboardShare,
    FileTransfer,
    // 车辆
    VehicleLocation,
    VehicleDiagnostics,
    VehicleClimate,
    VehicleLock,
    VehicleCharge,
    // 穿戴
    HeartRate,
    StepCount,
    SleepTracking,
    BloodOxygen,
    // 能源
    PowerConsumption,
    SolarProduction,
    BatteryLevel,
    // 清洁
    StartClean,
    CleaningMap,
    CleaningSchedule,
    // 3D 打印
    Print3DStart,
    Print3DStatus,
    Print3DTemperature,
    // 无人机
    DroneArm,
    DroneFly,
    DroneCamera,
    /// 自定义
    CustomCommand(String),
}

// ─── 能力集合 ───────────────────────────────────────────

/// 设备能力集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySet {
    capabilities: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn new(caps: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            capabilities: caps.into_iter().collect(),
        }
    }

    pub fn empty() -> Self {
        Self {
            capabilities: HashSet::new(),
        }
    }

    pub fn has(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.capabilities.iter()
    }

    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    pub fn merge(&self, other: &CapabilitySet) -> CapabilitySet {
        CapabilitySet {
            capabilities: self
                .capabilities
                .union(&other.capabilities)
                .cloned()
                .collect(),
        }
    }
}

/// 按设备类型返回默认能力
pub fn default_capabilities(device_type: &DeviceType) -> CapabilitySet {
    use Capability::*;
    match device_type {
        DeviceType::Light => CapabilitySet::new([Switch, Dimmer, ColorControl]),
        DeviceType::Thermostat => {
            CapabilitySet::new([TemperatureRead, TemperatureSet, HumidityRead])
        }
        DeviceType::Camera => CapabilitySet::new([VideoStream, VideoRecord, MotionDetect]),
        DeviceType::Lock => CapabilitySet::new([LockUnlock]),
        DeviceType::Sensor => CapabilitySet::new([TemperatureRead, HumidityRead]),
        DeviceType::Printer => CapabilitySet::new([Print, PrintStatus, ScanDocument]),
        DeviceType::Speaker => CapabilitySet::new([MediaPlayback, VolumeControl]),
        DeviceType::Nas => CapabilitySet::new([FileShare, StorageInfo]),
        DeviceType::Computer => {
            CapabilitySet::new([WakeOnLan, RemoteShell, ClipboardShare, FileTransfer])
        }
        DeviceType::Vehicle => CapabilitySet::new([
            VehicleLocation,
            VehicleDiagnostics,
            VehicleClimate,
            VehicleLock,
            VehicleCharge,
        ]),
        DeviceType::Wearable => {
            CapabilitySet::new([HeartRate, StepCount, SleepTracking, BloodOxygen])
        }
        DeviceType::SmartTv => {
            CapabilitySet::new([Switch, MediaPlayback, VolumeControl, ScreenMirror])
        }
        DeviceType::RobotVacuum => {
            CapabilitySet::new([StartClean, CleaningMap, CleaningSchedule, BatteryLevel])
        }
        DeviceType::EnergyMonitor => {
            CapabilitySet::new([PowerConsumption, SolarProduction, BatteryLevel])
        }
        DeviceType::Printer3D => {
            CapabilitySet::new([Print3DStart, Print3DStatus, Print3DTemperature])
        }
        DeviceType::Drone => CapabilitySet::new([DroneArm, DroneFly, DroneCamera, BatteryLevel]),
        DeviceType::Intercom => CapabilitySet::new([LockUnlock, MotionDetect, VideoStream]),
        _ => CapabilitySet::empty(),
    }
}

// ─── 测试 ───────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_type_from_str_known() {
        assert_eq!(DeviceType::from("light"), DeviceType::Light);
        assert_eq!(DeviceType::from("Printer"), DeviceType::Printer);
        assert_eq!(DeviceType::from("CAMERA"), DeviceType::Camera);
        assert_eq!(DeviceType::from("smart_tv"), DeviceType::SmartTv);
        assert_eq!(DeviceType::from("robot_vacuum"), DeviceType::RobotVacuum);
        assert_eq!(DeviceType::from("3d_printer"), DeviceType::Printer3D);
    }

    #[test]
    fn device_type_from_str_unknown() {
        assert_eq!(
            DeviceType::from("toaster"),
            DeviceType::Other("toaster".into())
        );
    }

    #[test]
    fn protocol_from_str_known() {
        assert_eq!(Protocol::from("mqtt"), Protocol::Mqtt);
        assert_eq!(Protocol::from("HomeKit"), Protocol::HomeKit);
        assert_eq!(Protocol::from("z-wave"), Protocol::ZWave);
        assert_eq!(Protocol::from("bluetooth"), Protocol::Ble);
        assert_eq!(Protocol::from("obd-ii"), Protocol::Obd2);
    }

    #[test]
    fn protocol_from_str_unknown() {
        assert_eq!(
            Protocol::from("custom_proto"),
            Protocol::Other("custom_proto".into())
        );
    }

    #[test]
    fn capability_set_operations() {
        let set1 = CapabilitySet::new([Capability::Switch, Capability::Dimmer]);
        let set2 = CapabilitySet::new([Capability::Dimmer, Capability::ColorControl]);
        assert!(set1.has(&Capability::Switch));
        assert!(!set1.has(&Capability::ColorControl));
        assert_eq!(set1.len(), 2);

        let merged = set1.merge(&set2);
        assert_eq!(merged.len(), 3);
        assert!(merged.has(&Capability::Switch));
        assert!(merged.has(&Capability::ColorControl));
    }

    #[test]
    fn capability_set_empty() {
        let empty = CapabilitySet::empty();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn default_capabilities_all_device_types() {
        // 每种已知设备类型都应有非空能力集
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
        ];
        for dt in &types {
            let caps = default_capabilities(dt);
            assert!(!caps.is_empty(), "{:?} 应有默认能力", dt);
        }
    }

    #[test]
    fn default_capabilities_unknown_is_empty() {
        let caps = default_capabilities(&DeviceType::Other("unknown".into()));
        assert!(caps.is_empty());
    }

    #[test]
    fn device_type_serde_roundtrip() {
        let dt = DeviceType::SmartTv;
        let json = serde_json::to_string(&dt).unwrap();
        let back: DeviceType = serde_json::from_str(&json).unwrap();
        assert_eq!(dt, back);
    }

    #[test]
    fn protocol_serde_roundtrip() {
        let p = Protocol::Mqtt;
        let json = serde_json::to_string(&p).unwrap();
        let back: Protocol = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}
