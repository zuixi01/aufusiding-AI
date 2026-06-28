//! OBD-II 诊断适配器
//!
//! 通过 ELM327 串口适配器读取车辆 OBD-II 诊断数据（只读）。
//! 支持标准 PID 查询、DTC 故障码读取。
//! 通过 SerialPort trait 抽象串口，支持 mock 测试。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::iot::control::DeviceController;
use crate::iot::vehicle::types::VehicleDiagnostics;

// ── Serial Port Abstraction ──────────────────────────────────────────

/// 串口抽象 trait（支持 mock 测试）
pub trait SerialPort: Send + Sync {
    fn write_command(&mut self, cmd: &str) -> anyhow::Result<()>;
    fn read_response(&mut self) -> anyhow::Result<String>;
}

/// Mock 串口（测试用）
pub struct MockSerialPort {
    responses: HashMap<String, String>,
    last_command: Option<String>,
}

impl MockSerialPort {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
            last_command: None,
        }
    }

    pub fn expect(mut self, command: &str, response: &str) -> Self {
        self.responses
            .insert(command.trim().to_uppercase(), response.to_string());
        self
    }
}

impl SerialPort for MockSerialPort {
    fn write_command(&mut self, cmd: &str) -> anyhow::Result<()> {
        self.last_command = Some(cmd.trim().to_uppercase());
        Ok(())
    }

    fn read_response(&mut self) -> anyhow::Result<String> {
        let cmd = self
            .last_command
            .take()
            .ok_or_else(|| anyhow::anyhow!("no command sent"))?;
        self.responses
            .get(&cmd)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no mock response for: {}", cmd))
    }
}
// ── OBD PID Definitions ──────────────────────────────────────────────

/// OBD PID 解析函数类型
type PidParser = fn(&[u8]) -> Option<f64>;

/// 标准 OBD PID 定义
struct ObdPid {
    mode: u8,
    pid: u8,
    name: &'static str,
    data_bytes: usize,
    parse: PidParser,
}

/// 标准 PID 集合
const STANDARD_PIDS: &[ObdPid] = &[
    ObdPid {
        mode: 0x01,
        pid: 0x0C,
        name: "rpm",
        data_bytes: 2,
        parse: |b| Some(((b[0] as f64) * 256.0 + b[1] as f64) / 4.0),
    },
    ObdPid {
        mode: 0x01,
        pid: 0x0D,
        name: "speed",
        data_bytes: 1,
        parse: |b| Some(b[0] as f64),
    },
    ObdPid {
        mode: 0x01,
        pid: 0x05,
        name: "coolant_temp",
        data_bytes: 1,
        parse: |b| Some(b[0] as f64 - 40.0),
    },
    ObdPid {
        mode: 0x01,
        pid: 0x2F,
        name: "fuel_level",
        data_bytes: 1,
        parse: |b| Some(b[0] as f64 * 100.0 / 255.0),
    },
    ObdPid {
        mode: 0x01,
        pid: 0x42,
        name: "battery_voltage",
        data_bytes: 2,
        parse: |b| Some(((b[0] as f64) * 256.0 + b[1] as f64) / 1000.0),
    },
];

// ── Pure Parsing Functions ───────────────────────────────────────────

/// 构建 ELM327 初始化命令序列
pub fn build_init_sequence() -> Vec<&'static str> {
    vec!["ATZ", "ATE0", "ATL0", "ATS0", "ATSP0"]
}

/// 构建 PID 请求字符串（如 mode=0x01, pid=0x0D → "010D"）
pub fn build_pid_request(mode: u8, pid: u8) -> String {
    format!("{:02X}{:02X}", mode, pid)
}

/// 解析 PID 响应的原始 hex 字节
///
/// 输入: "41 0D 3C" (mode 01, pid 0D 的响应)
/// 输出: Some(vec![0x3C])
pub fn parse_pid_response(raw: &str, expected_mode: u8, expected_pid: u8) -> Option<Vec<u8>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains("NO DATA") || trimmed.contains("ERROR") {
        return None;
    }

    let bytes: Vec<u8> = trimmed
        .split_whitespace()
        .filter_map(|s| u8::from_str_radix(s, 16).ok())
        .collect();

    // 响应格式: [response_mode] [pid] [data...]
    // response_mode = request_mode + 0x40
    let response_mode = expected_mode + 0x40;
    if bytes.len() < 2 || bytes[0] != response_mode || bytes[1] != expected_pid {
        return None;
    }

    Some(bytes[2..].to_vec())
}

/// DTC 两字节 → 标准故障码字符串
///
/// 高字节的高 2 位决定前缀: 00=P, 01=C, 10=B, 11=U
/// 示例: (0x03, 0x01) → "P0301"
pub fn dtc_bytes_to_code(high: u8, low: u8) -> String {
    let prefix = match (high >> 6) & 0x03 {
        0 => 'P',
        1 => 'C',
        2 => 'B',
        3 => 'U',
        _ => 'P',
    };
    let digit1 = (high >> 4) & 0x03;
    let digit2 = high & 0x0F;
    let digit3 = (low >> 4) & 0x0F;
    let digit4 = low & 0x0F;
    format!("{}{}{:X}{:X}{:X}", prefix, digit1, digit2, digit3, digit4)
}

/// 解析 DTC 响应
///
/// 输入: "43 01 03 01 04 20" (mode 03 响应)
/// 输出: ["P0301", "P0420"]
pub fn parse_dtc_response(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains("NO DATA") {
        return Vec::new();
    }

    let bytes: Vec<u8> = trimmed
        .split_whitespace()
        .filter_map(|s| u8::from_str_radix(s, 16).ok())
        .collect();

    // 首字节应为 0x43 (mode 03 + 0x40)
    if bytes.is_empty() || bytes[0] != 0x43 {
        return Vec::new();
    }

    let data = &bytes[1..];
    let mut codes = Vec::new();

    for chunk in data.chunks(2) {
        if chunk.len() == 2 && (chunk[0] != 0 || chunk[1] != 0) {
            codes.push(dtc_bytes_to_code(chunk[0], chunk[1]));
        }
    }

    codes
}

// ── ELM327 Communicator ──────────────────────────────────────────────

/// ELM327 通信器
pub struct Elm327 {
    port: Box<dyn SerialPort>,
}

impl Elm327 {
    pub fn new(port: Box<dyn SerialPort>) -> Self {
        Self { port }
    }

    /// 初始化 ELM327 适配器
    pub fn init(&mut self) -> anyhow::Result<()> {
        for cmd in build_init_sequence() {
            self.port.write_command(cmd)?;
            let resp = self.port.read_response()?;
            // ATZ 应返回 "ELM327 v..." 或 "OK"
            if cmd == "ATZ" && !resp.contains("ELM") && !resp.contains("OK") {
                anyhow::bail!("ELM327 init failed: unexpected response to ATZ: {}", resp);
            }
        }
        Ok(())
    }

    /// 查询单个 PID
    pub fn query_pid(&mut self, mode: u8, pid: u8) -> anyhow::Result<Option<Vec<u8>>> {
        let cmd = build_pid_request(mode, pid);
        self.port.write_command(&cmd)?;
        let resp = self.port.read_response()?;
        Ok(parse_pid_response(&resp, mode, pid))
    }

    /// 读取所有标准诊断数据
    pub fn read_all_diagnostics(&mut self) -> anyhow::Result<VehicleDiagnostics> {
        let mut diag = VehicleDiagnostics::default();

        for pid_def in STANDARD_PIDS {
            if let Ok(Some(data)) = self.query_pid(pid_def.mode, pid_def.pid) {
                if data.len() >= pid_def.data_bytes {
                    if let Some(value) = (pid_def.parse)(&data) {
                        match pid_def.name {
                            "rpm" => diag.rpm = Some(value),
                            "speed" => diag.speed_kmh = Some(value),
                            "coolant_temp" => diag.engine_temp_c = Some(value),
                            "fuel_level" => diag.fuel_level_pct = Some(value),
                            "battery_voltage" => diag.battery_voltage = Some(value),
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(diag)
    }

    /// 读取 DTC 故障码
    pub fn read_dtcs(&mut self) -> anyhow::Result<Vec<String>> {
        self.port.write_command("03")?;
        let resp = self.port.read_response()?;
        Ok(parse_dtc_response(&resp))
    }

    /// 读取完整诊断（含 DTC）
    pub fn read_full(&mut self) -> anyhow::Result<VehicleDiagnostics> {
        let mut diag = self.read_all_diagnostics()?;
        diag.dtc_codes = self.read_dtcs()?;
        Ok(diag)
    }
}

// ── OBD Adapter (DeviceController) ───────────────────────────────────

/// OBD-II 适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObdConfig {
    /// 串口路径 (e.g. "/dev/ttyUSB0" or "COM3")
    pub port: String,
    /// 波特率 (默认 9600)
    pub baud_rate: u32,
}

/// OBD-II 适配器（只读诊断）
pub struct ObdAdapter {
    vehicles: HashMap<String, ObdConfig>,
    /// 测试用：注入 mock 诊断数据
    mock_diagnostics: HashMap<String, VehicleDiagnostics>,
}

impl ObdAdapter {
    pub fn new() -> Self {
        Self {
            vehicles: HashMap::new(),
            mock_diagnostics: HashMap::new(),
        }
    }

    /// 注册车辆 OBD 连接
    pub fn register_vehicle(&mut self, device_id: &str, config: ObdConfig) {
        self.vehicles.insert(device_id.to_string(), config);
    }

    /// 注入 mock 诊断数据（测试用）
    pub fn inject_diagnostics(&mut self, device_id: &str, diag: VehicleDiagnostics) {
        self.mock_diagnostics.insert(device_id.to_string(), diag);
    }

    fn get_config(&self, device_id: &str) -> anyhow::Result<&ObdConfig> {
        self.vehicles
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown vehicle: {}", device_id))
    }

    /// 读取诊断数据
    fn read_diagnostics(&self, device_id: &str) -> anyhow::Result<VehicleDiagnostics> {
        // 优先使用注入的 mock 数据
        if let Some(diag) = self.mock_diagnostics.get(device_id) {
            return Ok(diag.clone());
        }

        // 真实实现: 打开串口 → Elm327::new → init → read_full
        // let port = serialport::new(&config.port, config.baud_rate).open()?;
        // let mut elm = Elm327::new(Box::new(RealSerialPort(port)));
        // elm.init()?;
        // elm.read_full()
        Ok(VehicleDiagnostics::default())
    }
}

#[async_trait]
impl DeviceController for ObdAdapter {
    async fn turn_on(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("OBD-II is read-only — cannot turn on vehicle")
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("OBD-II is read-only — cannot turn off vehicle")
    }

    async fn set_property(
        &self,
        _device_id: &str,
        _property: &str,
        _value: Value,
    ) -> anyhow::Result<()> {
        anyhow::bail!("OBD-II is read-only — cannot set properties")
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let _config = self.get_config(device_id)?;
        let diag = self.read_diagnostics(device_id)?;
        Ok(serde_json::to_value(diag)?)
    }

    fn supported_type(&self) -> &str {
        "Vehicle"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Pure function tests ──────────────────────────────────────────

    #[test]
    fn parse_speed_60kmh() {
        // 41 0D 3C → speed = 60
        let data = parse_pid_response("41 0D 3C", 0x01, 0x0D).unwrap();
        assert_eq!(data, vec![0x3C]);
        let speed = (STANDARD_PIDS[1].parse)(&data);
        assert_eq!(speed, Some(60.0));
    }

    #[test]
    fn parse_rpm_1726() {
        // 41 0C 1A F8 → rpm = (0x1A*256 + 0xF8)/4 = 1726.0
        let data = parse_pid_response("41 0C 1A F8", 0x01, 0x0C).unwrap();
        assert_eq!(data, vec![0x1A, 0xF8]);
        let rpm = (STANDARD_PIDS[0].parse)(&data);
        assert_eq!(rpm, Some(1726.0));
    }

    #[test]
    fn parse_coolant_temp_85c() {
        // 41 05 7D → temp = 0x7D - 40 = 85
        let data = parse_pid_response("41 05 7D", 0x01, 0x05).unwrap();
        let temp = (STANDARD_PIDS[2].parse)(&data);
        assert_eq!(temp, Some(85.0));
    }

    #[test]
    fn parse_fuel_level() {
        // 41 2F A0 → fuel = 0xA0 * 100 / 255 ≈ 62.7%
        let data = parse_pid_response("41 2F A0", 0x01, 0x2F).unwrap();
        let fuel = (STANDARD_PIDS[3].parse)(&data);
        let fuel_val = fuel.unwrap();
        assert!((fuel_val - 62.745).abs() < 0.1);
    }

    #[test]
    fn parse_battery_voltage() {
        // 41 42 31 2C → voltage = (0x31*256 + 0x2C)/1000 = 12.588
        let data = parse_pid_response("41 42 31 2C", 0x01, 0x42).unwrap();
        let voltage = (STANDARD_PIDS[4].parse)(&data);
        assert!((voltage.unwrap() - 12.588).abs() < 0.001);
    }

    #[test]
    fn parse_pid_response_no_data() {
        assert!(parse_pid_response("NO DATA", 0x01, 0x0D).is_none());
    }

    #[test]
    fn parse_pid_response_empty() {
        assert!(parse_pid_response("", 0x01, 0x0D).is_none());
    }

    #[test]
    fn parse_pid_response_error() {
        assert!(parse_pid_response("ERROR", 0x01, 0x0D).is_none());
    }

    #[test]
    fn parse_pid_response_wrong_mode() {
        // 期望 mode 01 的响应 (41)，但收到 42
        assert!(parse_pid_response("42 0D 3C", 0x01, 0x0D).is_none());
    }

    #[test]
    fn parse_pid_response_wrong_pid() {
        assert!(parse_pid_response("41 0C 3C", 0x01, 0x0D).is_none());
    }

    // ── DTC tests ────────────────────────────────────────────────────

    #[test]
    fn dtc_powertrain_p0301() {
        assert_eq!(dtc_bytes_to_code(0x03, 0x01), "P0301");
    }

    #[test]
    fn dtc_powertrain_p0420() {
        assert_eq!(dtc_bytes_to_code(0x04, 0x20), "P0420");
    }

    #[test]
    fn dtc_chassis_c0301() {
        // 高字节 01_00_0011 = 0x43
        assert_eq!(dtc_bytes_to_code(0x43, 0x01), "C0301");
    }

    #[test]
    fn dtc_body_b0301() {
        // 高字节 10_00_0011 = 0x83
        assert_eq!(dtc_bytes_to_code(0x83, 0x01), "B0301");
    }

    #[test]
    fn dtc_network_u0301() {
        // 高字节 11_00_0011 = 0xC3
        assert_eq!(dtc_bytes_to_code(0xC3, 0x01), "U0301");
    }

    #[test]
    fn parse_dtc_response_two_codes() {
        let codes = parse_dtc_response("43 03 01 04 20");
        assert_eq!(codes, vec!["P0301", "P0420"]);
    }

    #[test]
    fn parse_dtc_response_no_codes() {
        let codes = parse_dtc_response("43 00 00");
        assert!(codes.is_empty());
    }

    #[test]
    fn parse_dtc_response_no_data() {
        let codes = parse_dtc_response("NO DATA");
        assert!(codes.is_empty());
    }

    #[test]
    fn parse_dtc_response_empty() {
        let codes = parse_dtc_response("");
        assert!(codes.is_empty());
    }

    // ── Build functions ──────────────────────────────────────────────

    #[test]
    fn build_init_sequence_correct() {
        let seq = build_init_sequence();
        assert_eq!(seq, vec!["ATZ", "ATE0", "ATL0", "ATS0", "ATSP0"]);
    }

    #[test]
    fn build_pid_request_speed() {
        assert_eq!(build_pid_request(0x01, 0x0D), "010D");
    }

    #[test]
    fn build_pid_request_rpm() {
        assert_eq!(build_pid_request(0x01, 0x0C), "010C");
    }

    #[test]
    fn build_pid_request_dtc() {
        assert_eq!(build_pid_request(0x03, 0x00), "0300");
    }

    // ── MockSerialPort + Elm327 tests ────────────────────────────────

    #[test]
    fn mock_serial_port_responds() {
        let mut port = MockSerialPort::new().expect("010D", "41 0D 3C");
        port.write_command("010D").unwrap();
        let resp = port.read_response().unwrap();
        assert_eq!(resp, "41 0D 3C");
    }

    #[test]
    fn mock_serial_port_unknown_command() {
        let mut port = MockSerialPort::new();
        port.write_command("XXXX").unwrap();
        assert!(port.read_response().is_err());
    }

    #[test]
    fn elm327_init_success() {
        let port = MockSerialPort::new()
            .expect("ATZ", "ELM327 v2.1")
            .expect("ATE0", "OK")
            .expect("ATL0", "OK")
            .expect("ATS0", "OK")
            .expect("ATSP0", "OK");
        let mut elm = Elm327::new(Box::new(port));
        assert!(elm.init().is_ok());
    }

    #[test]
    fn elm327_init_fails_on_bad_atz() {
        let port = MockSerialPort::new().expect("ATZ", "GARBAGE");
        let mut elm = Elm327::new(Box::new(port));
        assert!(elm.init().is_err());
    }

    #[test]
    fn elm327_query_pid_speed() {
        let port = MockSerialPort::new().expect("010D", "41 0D 3C");
        let mut elm = Elm327::new(Box::new(port));
        let data = elm.query_pid(0x01, 0x0D).unwrap();
        assert_eq!(data, Some(vec![0x3C]));
    }

    #[test]
    fn elm327_read_dtcs() {
        let port = MockSerialPort::new().expect("03", "43 03 01 04 20");
        let mut elm = Elm327::new(Box::new(port));
        let codes = elm.read_dtcs().unwrap();
        assert_eq!(codes, vec!["P0301", "P0420"]);
    }

    #[test]
    fn elm327_read_all_diagnostics() {
        let port = MockSerialPort::new()
            .expect("010C", "41 0C 1A F8") // RPM = 1726
            .expect("010D", "41 0D 3C") // Speed = 60
            .expect("0105", "41 05 7D") // Temp = 85
            .expect("012F", "41 2F A0") // Fuel ≈ 62.7%
            .expect("0142", "41 42 31 2C"); // Voltage = 12.588
        let mut elm = Elm327::new(Box::new(port));
        let diag = elm.read_all_diagnostics().unwrap();

        assert_eq!(diag.rpm, Some(1726.0));
        assert_eq!(diag.speed_kmh, Some(60.0));
        assert_eq!(diag.engine_temp_c, Some(85.0));
        assert!((diag.fuel_level_pct.unwrap() - 62.745).abs() < 0.1);
        assert!((diag.battery_voltage.unwrap() - 12.588).abs() < 0.001);
    }

    #[test]
    fn elm327_read_diagnostics_partial() {
        // 只有 speed 有响应，其他 NO DATA
        let port = MockSerialPort::new()
            .expect("010C", "NO DATA")
            .expect("010D", "41 0D 50") // Speed = 80
            .expect("0105", "NO DATA")
            .expect("012F", "NO DATA")
            .expect("0142", "NO DATA");
        let mut elm = Elm327::new(Box::new(port));
        let diag = elm.read_all_diagnostics().unwrap();

        assert!(diag.rpm.is_none());
        assert_eq!(diag.speed_kmh, Some(80.0));
        assert!(diag.engine_temp_c.is_none());
        assert!(diag.fuel_level_pct.is_none());
        assert!(diag.battery_voltage.is_none());
    }

    #[test]
    fn elm327_read_full_with_dtcs() {
        let port = MockSerialPort::new()
            .expect("010C", "41 0C 0B B8") // RPM = 750
            .expect("010D", "41 0D 00") // Speed = 0
            .expect("0105", "41 05 50") // Temp = 40
            .expect("012F", "41 2F FF") // Fuel = 100%
            .expect("0142", "41 42 31 2C") // Voltage
            .expect("03", "43 03 01"); // DTC: P0301
        let mut elm = Elm327::new(Box::new(port));
        let diag = elm.read_full().unwrap();

        assert_eq!(diag.speed_kmh, Some(0.0));
        assert_eq!(diag.dtc_codes, vec!["P0301"]);
    }

    // ── ObdAdapter (DeviceController) tests ──────────────────────────

    fn setup() -> ObdAdapter {
        let mut adapter = ObdAdapter::new();
        adapter.register_vehicle(
            "car-obd-001",
            ObdConfig {
                port: "/dev/ttyUSB0".into(),
                baud_rate: 9600,
            },
        );
        adapter.inject_diagnostics(
            "car-obd-001",
            VehicleDiagnostics {
                speed_kmh: Some(60.0),
                rpm: Some(2500.0),
                fuel_level_pct: Some(65.0),
                battery_voltage: Some(12.6),
                engine_temp_c: Some(85.0),
                dtc_codes: vec!["P0301".into()],
            },
        );
        adapter
    }

    #[test]
    fn supported_type_is_vehicle() {
        assert_eq!(ObdAdapter::new().supported_type(), "Vehicle");
    }

    #[tokio::test]
    async fn turn_on_rejected() {
        let adapter = setup();
        let err = adapter.turn_on("car-obd-001").await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn turn_off_rejected() {
        let adapter = setup();
        let err = adapter.turn_off("car-obd-001").await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn set_property_rejected() {
        let adapter = setup();
        let err = adapter
            .set_property("car-obd-001", "speed", Value::Null)
            .await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn get_state_returns_diagnostics() {
        let adapter = setup();
        let state = adapter.get_state("car-obd-001").await.unwrap();
        assert_eq!(state["speed_kmh"], 60.0);
        assert_eq!(state["rpm"], 2500.0);
        assert_eq!(state["fuel_level_pct"], 65.0);
        assert_eq!(state["battery_voltage"], 12.6);
        assert_eq!(state["engine_temp_c"], 85.0);
        assert_eq!(state["dtc_codes"][0], "P0301");
    }

    #[tokio::test]
    async fn unknown_vehicle_errors() {
        let adapter = ObdAdapter::new();
        assert!(adapter.get_state("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn get_state_without_mock_returns_default() {
        let mut adapter = ObdAdapter::new();
        adapter.register_vehicle(
            "car-002",
            ObdConfig {
                port: "/dev/ttyUSB1".into(),
                baud_rate: 9600,
            },
        );
        // 没有注入 mock 数据，应返回 default
        let state = adapter.get_state("car-002").await.unwrap();
        assert!(state["speed_kmh"].is_null());
        assert!(state["rpm"].is_null());
    }
}
