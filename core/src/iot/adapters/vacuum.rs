//! 扫地机器人适配器
//!
//! 支持 Roborock 扫地机器人通过 Roborock Cloud API。
//! 能力: Cleaning, DockReturn, RoomSelect, FanSpeed, WaterLevel

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::iot::control::DeviceController;

const DEFAULT_BASE_URL: &str = "https://api.roborock.com";

// ---------------------------------------------------------------------------
// Config & data types
// ---------------------------------------------------------------------------

/// Roborock Cloud API 配置
#[derive(Debug, Clone)]
pub struct RoborockConfig {
    pub access_token: String,
}

/// Roborock 设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoborockDevice {
    pub device_id: String,
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub online: bool,
}

/// 设备状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub state: u8,           // 5=cleaning, 6=returning, 8=charging, etc.
    pub battery: u8,         // 0-100
    pub fan_power: u16,      // 101=quiet, 102=balanced, 103=turbo, 104=max
    pub clean_area: u32,     // m² × 100
    pub clean_time: u32,     // seconds
    pub error_code: u16,
}

// ---------------------------------------------------------------------------
// API response wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RoborockResponse<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Roborock Cloud API 适配器
pub struct RoborockAdapter {
    client: reqwest::Client,
    base_url: String,
    access_token: String,
    devices: HashMap<String, RoborockDevice>,
}

impl RoborockAdapter {
    /// 创建新的适配器
    pub fn new(config: RoborockConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            access_token: config.access_token,
            devices: HashMap::new(),
        }
    }

    /// 覆盖 base URL (用于测试)
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// 列出所有设备
    pub async fn list_devices(&mut self) -> anyhow::Result<Vec<RoborockDevice>> {
        let url = self.endpoint("api/v1/user/devices");

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("list_devices failed ({}): {}", status.as_u16(), text);
        }

        let parsed: RoborockResponse<Vec<RoborockDevice>> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!("list_devices error (code {}): {}", parsed.code, parsed.message);
        }

        let devices = parsed.data.unwrap_or_default();
        // Cache devices
        for device in &devices {
            self.devices.insert(device.device_id.clone(), device.clone());
        }

        Ok(devices)
    }

    /// 获取设备状态
    pub async fn get_status(&self, device_id: &str) -> anyhow::Result<DeviceStatus> {
        let url = self.endpoint(&format!("api/v1/device/{}/status", device_id));

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if status.as_u16() == 404 {
            anyhow::bail!("device not found: {}", device_id);
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("get_status failed ({}): {}", status.as_u16(), text);
        }

        let parsed: RoborockResponse<DeviceStatus> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!("get_status error (code {}): {}", parsed.code, parsed.message);
        }

        parsed.data.ok_or_else(|| anyhow::anyhow!("no status data returned"))
    }

    /// 发送命令
    pub async fn send_command(
        &self,
        device_id: &str,
        method: &str,
        params: Option<Value>,
    ) -> anyhow::Result<Value> {
        let url = self.endpoint(&format!("api/v1/device/{}/command", device_id));

        let mut body = json!({ "method": method });
        if let Some(p) = params {
            body["params"] = p;
        }

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if status.as_u16() == 404 {
            anyhow::bail!("device not found: {}", device_id);
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("send_command failed ({}): {}", status.as_u16(), text);
        }

        let parsed: RoborockResponse<Value> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!("send_command error (code {}): {}", parsed.code, parsed.message);
        }

        Ok(parsed.data.unwrap_or(json!({})))
    }

    /// 开始清扫
    pub async fn start_clean(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, "app_start", None).await?;
        Ok(())
    }

    /// 停止清扫
    pub async fn stop_clean(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, "app_stop", None).await?;
        Ok(())
    }

    /// 暂停清扫
    pub async fn pause(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, "app_pause", None).await?;
        Ok(())
    }

    /// 回充
    pub async fn return_dock(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, "app_charge", None).await?;
        Ok(())
    }

    /// 定点清扫
    pub async fn spot_clean(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, "app_spot", None).await?;
        Ok(())
    }

    /// 房间清扫
    pub async fn room_clean(&self, device_id: &str, room_ids: Vec<u32>) -> anyhow::Result<()> {
        self.send_command(device_id, "app_segment_clean", Some(json!(room_ids)))
            .await?;
        Ok(())
    }

    /// 设置吸力
    pub async fn set_fan_power(&self, device_id: &str, level: u16) -> anyhow::Result<()> {
        if ![101, 102, 103, 104].contains(&level) {
            anyhow::bail!("invalid fan_power level: {} (must be 101-104)", level);
        }
        self.send_command(device_id, "set_custom_mode", Some(json!([level])))
            .await?;
        Ok(())
    }

    /// 设置拖地水量
    pub async fn set_water_level(&self, device_id: &str, level: u16) -> anyhow::Result<()> {
        if ![200, 201, 202].contains(&level) {
            anyhow::bail!("invalid water_level: {} (must be 200-202)", level);
        }
        self.send_command(device_id, "set_water_box_custom_mode", Some(json!([level])))
            .await?;
        Ok(())
    }
}

#[async_trait]
impl DeviceController for RoborockAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.start_clean(device_id).await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        self.return_dock(device_id).await
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        match property {
            "fan_power" => {
                let level = value
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("fan_power must be a number"))?
                    as u16;
                self.set_fan_power(device_id, level).await
            }
            "water_level" => {
                let level = value
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("water_level must be a number"))?
                    as u16;
                self.set_water_level(device_id, level).await
            }
            "rooms" => {
                let rooms = value
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("rooms must be an array"))?
                    .iter()
                    .map(|v| {
                        v.as_u64()
                            .ok_or_else(|| anyhow::anyhow!("room_id must be a number"))
                            .map(|n| n as u32)
                    })
                    .collect::<Result<Vec<u32>, _>>()?;
                self.room_clean(device_id, rooms).await
            }
            _ => anyhow::bail!("unsupported vacuum property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let status = self.get_status(device_id).await?;

        let state_str = match status.state {
            5 => "cleaning",
            6 => "returning",
            8 => "charging",
            10 => "paused",
            _ => "idle",
        };

        Ok(json!({
            "status": state_str,
            "state_code": status.state,
            "battery_pct": status.battery,
            "fan_power": status.fan_power,
            "area_cleaned_m2": (status.clean_area as f64) / 100.0,
            "clean_time_sec": status.clean_time,
            "error_code": status.error_code,
        }))
    }

    fn supported_type(&self) -> &str {
        "RobotVacuum"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn ok_response<T: Serialize>(data: T) -> Value {
        json!({
            "code": 0,
            "message": "success",
            "data": data
        })
    }

    fn error_response(code: i32, message: &str) -> Value {
        json!({
            "code": code,
            "message": message,
            "data": null
        })
    }

    fn setup(server: &MockServer) -> RoborockAdapter {
        RoborockAdapter::new(RoborockConfig {
            access_token: "test-token".to_string(),
        })
        .with_base_url(&server.base_url())
    }

    // 1. list_devices success
    #[tokio::test]
    async fn list_devices_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/user/devices")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(ok_response(json!([
                {
                    "device_id": "vac-001",
                    "name": "Living Room",
                    "model": "roborock.vacuum.s7",
                    "online": true
                }
            ])));
        });

        let mut adapter = setup(&server);
        let devices = adapter.list_devices().await.unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, "vac-001");
        assert_eq!(devices[0].name, "Living Room");
        assert!(devices[0].online);
        mock.assert();
    }

    // 2. list_devices unauthorized
    #[tokio::test]
    async fn list_devices_unauthorized() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/v1/user/devices");
            then.status(401);
        });

        let mut adapter = setup(&server);
        let err = adapter.list_devices().await.unwrap_err();
        assert!(err.to_string().contains("unauthorized"));
    }

    // 3. list_devices API error code
    #[tokio::test]
    async fn list_devices_api_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/v1/user/devices");
            then.status(200)
                .json_body(error_response(-1001, "internal error"));
        });

        let mut adapter = setup(&server);
        let err = adapter.list_devices().await.unwrap_err();
        assert!(err.to_string().contains("internal error"));
    }

    // 4. get_status success
    #[tokio::test]
    async fn get_status_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/device/vac-001/status")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(ok_response(json!({
                "state": 8,
                "battery": 95,
                "fan_power": 102,
                "clean_area": 4500,
                "clean_time": 1800,
                "error_code": 0
            })));
        });

        let adapter = setup(&server);
        let status = adapter.get_status("vac-001").await.unwrap();

        assert_eq!(status.state, 8);
        assert_eq!(status.battery, 95);
        assert_eq!(status.fan_power, 102);
        assert_eq!(status.clean_area, 4500);
        assert_eq!(status.clean_time, 1800);
        mock.assert();
    }

    // 5. get_status device not found
    #[tokio::test]
    async fn get_status_device_not_found() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/v1/device/unknown/status");
            then.status(404);
        });

        let adapter = setup(&server);
        let err = adapter.get_status("unknown").await.unwrap_err();
        assert!(err.to_string().contains("device not found"));
    }

    // 6. send_command success
    #[tokio::test]
    async fn send_command_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"method": "app_start"}));
            then.status(200).json_body(ok_response(json!({"result": "ok"})));
        });

        let adapter = setup(&server);
        adapter.send_command("vac-001", "app_start", None).await.unwrap();
        mock.assert();
    }

    // 7. send_command with params
    #[tokio::test]
    async fn send_command_with_params() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "set_custom_mode", "params": [103]}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter
            .send_command("vac-001", "set_custom_mode", Some(json!([103])))
            .await
            .unwrap();
        mock.assert();
    }

    // 8. start_clean
    #[tokio::test]
    async fn start_clean_sends_app_start() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_start"}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter.start_clean("vac-001").await.unwrap();
        mock.assert();
    }

    // 9. stop_clean
    #[tokio::test]
    async fn stop_clean_sends_app_stop() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_stop"}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter.stop_clean("vac-001").await.unwrap();
        mock.assert();
    }

    // 10. pause
    #[tokio::test]
    async fn pause_sends_app_pause() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_pause"}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter.pause("vac-001").await.unwrap();
        mock.assert();
    }

    // 11. return_dock
    #[tokio::test]
    async fn return_dock_sends_app_charge() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_charge"}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter.return_dock("vac-001").await.unwrap();
        mock.assert();
    }

    // 12. spot_clean
    #[tokio::test]
    async fn spot_clean_sends_app_spot() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_spot"}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter.spot_clean("vac-001").await.unwrap();
        mock.assert();
    }

    // 13. room_clean
    #[tokio::test]
    async fn room_clean_sends_segment_clean() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_segment_clean", "params": [1, 2, 3]}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter.room_clean("vac-001", vec![1, 2, 3]).await.unwrap();
        mock.assert();
    }

    // 14. set_fan_power valid levels
    #[tokio::test]
    async fn set_fan_power_valid_levels() {
        let server = MockServer::start();

        for level in [101, 102, 103, 104] {
            let mock = server.mock(|when, then| {
                when.method(POST)
                    .path("/api/v1/device/vac-001/command")
                    .json_body(json!({"method": "set_custom_mode", "params": [level]}));
                then.status(200).json_body(ok_response(json!({})));
            });

            let adapter = setup(&server);
            adapter.set_fan_power("vac-001", level).await.unwrap();
            mock.assert();
        }
    }

    // 15. set_fan_power invalid level
    #[tokio::test]
    async fn set_fan_power_invalid_level() {
        let server = MockServer::start();
        let adapter = setup(&server);

        let err = adapter.set_fan_power("vac-001", 999).await.unwrap_err();
        assert!(err.to_string().contains("invalid fan_power"));
    }

    // 16. set_water_level valid levels
    #[tokio::test]
    async fn set_water_level_valid_levels() {
        let server = MockServer::start();

        for level in [200, 201, 202] {
            let mock = server.mock(|when, then| {
                when.method(POST)
                    .path("/api/v1/device/vac-001/command")
                    .json_body(json!({"method": "set_water_box_custom_mode", "params": [level]}));
                then.status(200).json_body(ok_response(json!({})));
            });

            let adapter = setup(&server);
            adapter.set_water_level("vac-001", level).await.unwrap();
            mock.assert();
        }
    }

    // 17. set_water_level invalid level
    #[tokio::test]
    async fn set_water_level_invalid_level() {
        let server = MockServer::start();
        let adapter = setup(&server);

        let err = adapter.set_water_level("vac-001", 999).await.unwrap_err();
        assert!(err.to_string().contains("invalid water_level"));
    }

    // 18. DeviceController turn_on calls start_clean
    #[tokio::test]
    async fn device_controller_turn_on() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_start"}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter.turn_on("vac-001").await.unwrap();
        mock.assert();
    }

    // 19. DeviceController turn_off calls return_dock
    #[tokio::test]
    async fn device_controller_turn_off() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_charge"}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter.turn_off("vac-001").await.unwrap();
        mock.assert();
    }

    // 20. DeviceController set_property fan_power
    #[tokio::test]
    async fn device_controller_set_fan_power() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "set_custom_mode", "params": [103]}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter
            .set_property("vac-001", "fan_power", json!(103))
            .await
            .unwrap();
        mock.assert();
    }

    // 21. DeviceController set_property water_level
    #[tokio::test]
    async fn device_controller_set_water_level() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "set_water_box_custom_mode", "params": [201]}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter
            .set_property("vac-001", "water_level", json!(201))
            .await
            .unwrap();
        mock.assert();
    }

    // 22. DeviceController set_property rooms
    #[tokio::test]
    async fn device_controller_set_rooms() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v1/device/vac-001/command")
                .json_body(json!({"method": "app_segment_clean", "params": [10, 20]}));
            then.status(200).json_body(ok_response(json!({})));
        });

        let adapter = setup(&server);
        adapter
            .set_property("vac-001", "rooms", json!([10, 20]))
            .await
            .unwrap();
        mock.assert();
    }

    // 23. DeviceController get_state returns formatted status
    #[tokio::test]
    async fn device_controller_get_state() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/v1/device/vac-001/status");
            then.status(200).json_body(ok_response(json!({
                "state": 5,
                "battery": 80,
                "fan_power": 103,
                "clean_area": 3250,
                "clean_time": 1200,
                "error_code": 0
            })));
        });

        let adapter = setup(&server);
        let state = adapter.get_state("vac-001").await.unwrap();

        assert_eq!(state["status"], "cleaning");
        assert_eq!(state["state_code"], 5);
        assert_eq!(state["battery_pct"], 80);
        assert_eq!(state["fan_power"], 103);
        assert_eq!(state["area_cleaned_m2"], 32.5);
        assert_eq!(state["clean_time_sec"], 1200);
        assert_eq!(state["error_code"], 0);
    }

    // 24. DeviceController supported_type
    #[test]
    fn device_controller_supported_type() {
        let server = MockServer::start();
        let adapter = setup(&server);
        assert_eq!(adapter.supported_type(), "RobotVacuum");
    }

    // 25. list_devices caches devices
    #[tokio::test]
    async fn list_devices_caches_devices() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/v1/user/devices");
            then.status(200).json_body(ok_response(json!([
                {
                    "device_id": "vac-001",
                    "name": "Test",
                    "model": "s7",
                    "online": true
                }
            ])));
        });

        let mut adapter = setup(&server);
        adapter.list_devices().await.unwrap();

        // Verify device is cached
        assert!(adapter.devices.contains_key("vac-001"));
        assert_eq!(adapter.devices.get("vac-001").unwrap().name, "Test");
    }
}
