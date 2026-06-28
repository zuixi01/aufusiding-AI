//! 车辆数据类型
//!
//! OBD-II 诊断数据、远程车辆状态和命令定义。

use serde::{Deserialize, Serialize};

/// 车辆诊断数据（OBD-II）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VehicleDiagnostics {
    /// 车速 (km/h)
    pub speed_kmh: Option<f64>,
    /// 发动机转速
    pub rpm: Option<f64>,
    /// 油量百分比
    pub fuel_level_pct: Option<f64>,
    /// 电池电压
    pub battery_voltage: Option<f64>,
    /// 发动机温度 (°C)
    pub engine_temp_c: Option<f64>,
    /// 故障码 (DTC)
    pub dtc_codes: Vec<String>,
}

/// 车辆状态（远程 API）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VehicleState {
    /// 是否锁定
    pub locked: bool,
    /// 空调是否开启
    pub climate_on: bool,
    /// 电池电量百分比
    pub charge_level_pct: Option<f64>,
    /// 位置 (lat, lng)
    pub location: Option<(f64, f64)>,
    /// 里程 (km)
    pub odometer_km: Option<f64>,
    /// 是否正在充电
    pub charging: bool,
    /// 续航里程 (km)
    pub range_km: Option<f64>,
}

/// 车辆命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VehicleCommand {
    /// 锁车
    Lock,
    /// 解锁
    Unlock,
    /// 开启空调
    StartClimate,
    /// 关闭空调
    StopClimate,
    /// 鸣笛
    Honk,
    /// 闪灯
    Flash,
    /// 打开后备箱
    OpenTrunk,
    /// 设置充电上限 (百分比)
    SetChargeLimit(u8),
    /// 开始充电
    StartCharging,
    /// 停止充电
    StopCharging,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_serde_roundtrip() {
        let diag = VehicleDiagnostics {
            speed_kmh: Some(60.0),
            rpm: Some(2500.0),
            fuel_level_pct: Some(75.0),
            battery_voltage: Some(12.6),
            engine_temp_c: Some(90.0),
            dtc_codes: vec!["P0301".to_string(), "P0420".to_string()],
        };
        let json = serde_json::to_string(&diag).unwrap();
        let restored: VehicleDiagnostics = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.speed_kmh, Some(60.0));
        assert_eq!(restored.dtc_codes.len(), 2);
        assert_eq!(restored.dtc_codes[0], "P0301");
    }

    #[test]
    fn diagnostics_default_is_empty() {
        let diag = VehicleDiagnostics::default();
        assert!(diag.speed_kmh.is_none());
        assert!(diag.dtc_codes.is_empty());
    }

    #[test]
    fn vehicle_state_serde_roundtrip() {
        let state = VehicleState {
            locked: true,
            climate_on: false,
            charge_level_pct: Some(80.0),
            location: Some((37.7749, -122.4194)),
            odometer_km: Some(15000.0),
            charging: true,
            range_km: Some(250.0),
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: VehicleState = serde_json::from_str(&json).unwrap();
        assert!(restored.locked);
        assert!(!restored.climate_on);
        assert_eq!(restored.charge_level_pct, Some(80.0));
        assert_eq!(restored.location, Some((37.7749, -122.4194)));
        assert!(restored.charging);
    }

    #[test]
    fn vehicle_state_default() {
        let state = VehicleState::default();
        assert!(!state.locked);
        assert!(!state.climate_on);
        assert!(!state.charging);
        assert!(state.location.is_none());
    }

    #[test]
    fn vehicle_command_serde_roundtrip() {
        let commands = vec![
            VehicleCommand::Lock,
            VehicleCommand::Unlock,
            VehicleCommand::StartClimate,
            VehicleCommand::StopClimate,
            VehicleCommand::Honk,
            VehicleCommand::Flash,
            VehicleCommand::OpenTrunk,
            VehicleCommand::SetChargeLimit(80),
            VehicleCommand::StartCharging,
            VehicleCommand::StopCharging,
        ];
        for cmd in &commands {
            let json = serde_json::to_string(cmd).unwrap();
            let restored: VehicleCommand = serde_json::from_str(&json).unwrap();
            // Verify roundtrip produces valid JSON and deserializes
            let json2 = serde_json::to_string(&restored).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn set_charge_limit_preserves_value() {
        let cmd = VehicleCommand::SetChargeLimit(90);
        let json = serde_json::to_string(&cmd).unwrap();
        let restored: VehicleCommand = serde_json::from_str(&json).unwrap();
        match restored {
            VehicleCommand::SetChargeLimit(v) => assert_eq!(v, 90),
            _ => panic!("expected SetChargeLimit"),
        }
    }
}
