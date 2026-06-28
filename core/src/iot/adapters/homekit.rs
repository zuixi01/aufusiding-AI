//! Apple HomeKit 适配器
//!
//! 通过 HomeKit Accessory Protocol (HAP) over IP 控制 Apple 生态设备。
//! 支持配件发现、配对、特征读写。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::iot::control::DeviceController;

#[allow(dead_code)]
const DEFAULT_BASE_URL: &str = "http://localhost:51826";

// ── HAP 数据结构 ────────────────────────────────────────────────────

/// HomeKit 配件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HapAccessory {
    pub aid: u64,
    pub services: Vec<HapService>,
}

/// HomeKit 服务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HapService {
    pub iid: u64,
    pub r#type: String,
    pub characteristics: Vec<HapCharacteristic>,
}

/// HomeKit 特征
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HapCharacteristic {
    pub iid: u64,
    pub r#type: String,
    pub value: Option<Value>,
    pub perms: Vec<String>,
    pub format: String,
}

/// HAP 特征类型常量
pub mod characteristic_types {
    pub const ON: &str = "25";
    pub const BRIGHTNESS: &str = "8";
    pub const COLOR_TEMPERATURE: &str = "CE";
    pub const TARGET_TEMPERATURE: &str = "35";
    pub const CURRENT_TEMPERATURE: &str = "11";
    pub const TARGET_HEATING_COOLING: &str = "33";
    pub const LOCK_TARGET: &str = "1E";
    pub const LOCK_CURRENT: &str = "1D";
    pub const ACTIVE: &str = "B0";
}

/// HomeKit 配件注册信息
struct HomeKitEntry {
    aid: u64,
    name: String,
    accessory_type: String,
}

/// HomeKit 适配器
pub struct HomeKitAdapter {
    client: reqwest::Client,
    base_url: String,
    accessories: HashMap<String, HomeKitEntry>,
}

impl HomeKitAdapter {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            accessories: HashMap::new(),
        }
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    pub fn register_accessory(
        &mut self,
        device_id: &str,
        aid: u64,
        name: &str,
        accessory_type: &str,
    ) {
        self.accessories.insert(
            device_id.to_string(),
            HomeKitEntry {
                aid,
                name: name.to_string(),
                accessory_type: accessory_type.to_string(),
            },
        );
    }

    fn get_entry(&self, device_id: &str) -> anyhow::Result<&HomeKitEntry> {
        self.accessories
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown HomeKit accessory: {}", device_id))
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// 获取所有配件
    pub async fn get_accessories(&self) -> anyhow::Result<Vec<HapAccessory>> {
        let url = self.endpoint("/accessories");
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("get accessories failed ({}): {}", status, body);
        }
        let body: Value = resp.json().await?;
        let accessories: Vec<HapAccessory> =
            serde_json::from_value(body["accessories"].clone()).unwrap_or_default();
        Ok(accessories)
    }

    /// 读取特征值
    pub async fn get_characteristic(&self, aid: u64, iid: u64) -> anyhow::Result<Value> {
        let url = self.endpoint(&format!("/characteristics?id={}.{}", aid, iid));
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("get characteristic failed: {}", resp.status());
        }
        let body: Value = resp.json().await?;
        let chars = body["characteristics"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("invalid response"))?;
        chars
            .first()
            .and_then(|c| c.get("value").cloned())
            .ok_or_else(|| anyhow::anyhow!("no value in response"))
    }

    /// 写入特征值
    pub async fn set_characteristic(&self, aid: u64, iid: u64, value: Value) -> anyhow::Result<()> {
        let url = self.endpoint("/characteristics");
        let body = json!({
            "characteristics": [{"aid": aid, "iid": iid, "value": value}]
        });
        let resp = self.client.put(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("set characteristic failed ({}): {}", status, err_body);
        }
        Ok(())
    }
}

#[async_trait]
impl DeviceController for HomeKitAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let entry = self.get_entry(device_id)?;
        self.set_characteristic(entry.aid, 8, json!(true)).await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let entry = self.get_entry(device_id)?;
        self.set_characteristic(entry.aid, 8, json!(false)).await
    }

    async fn set_property(&self, device_id: &str, property: &str, value: Value) -> anyhow::Result<()> {
        let entry = self.get_entry(device_id)?;
        let iid: u64 = match property {
            "brightness" => 9,
            "color_temperature" => 10,
            "target_temperature" => 11,
            "hvac_mode" => 12,
            "lock" => 13,
            _ => anyhow::bail!("unsupported HomeKit property: {}", property),
        };
        self.set_characteristic(entry.aid, iid, value).await
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let entry = self.get_entry(device_id)?;
        let on_value = self.get_characteristic(entry.aid, 8).await.ok();
        Ok(json!({
            "aid": entry.aid,
            "name": entry.name,
            "type": entry.accessory_type,
            "on": on_value,
        }))
    }

    fn supported_type(&self) -> &str {
        "HomeKit"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn setup(server: &MockServer) -> HomeKitAdapter {
        let mut adapter = HomeKitAdapter::new(DEFAULT_BASE_URL).with_base_url(&server.base_url());
        adapter.register_accessory("light-1", 1, "Living Room Light", "light");
        adapter.register_accessory("thermostat-1", 2, "Thermostat", "climate");
        adapter
    }

    #[tokio::test]
    async fn get_accessories_sends_get() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/accessories");
            then.status(200).json_body(json!({"accessories": []}));
        });
        let adapter = setup(&server);
        let result = adapter.get_accessories().await.unwrap();
        assert!(result.is_empty());
        mock.assert();
    }

    #[tokio::test]
    async fn get_accessories_parses_response() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/accessories");
            then.status(200).json_body(json!({"accessories": [{
                "aid": 1,
                "services": [{"iid": 1, "type": "3E", "characteristics": []}]
            }]}));
        });
        let adapter = setup(&server);
        let result = adapter.get_accessories().await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].aid, 1);
    }

    // 3. get_characteristic sends correct GET with aid.iid format
    #[tokio::test]
    async fn get_characteristic_sends_correct_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/characteristics").query_param("id", "1.8");
            then.status(200).json_body(json!({
                "characteristics": [{"aid": 1, "iid": 8, "value": true}]
            }));
        });
        let adapter = setup(&server);
        adapter.get_characteristic(1, 8).await.unwrap();
        mock.assert();
    }

    // 4. get_characteristic parses value from response
    #[tokio::test]
    async fn get_characteristic_parses_value() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/characteristics").query_param("id", "1.9");
            then.status(200).json_body(json!({
                "characteristics": [{"aid": 1, "iid": 9, "value": 75}]
            }));
        });
        let adapter = setup(&server);
        let val = adapter.get_characteristic(1, 9).await.unwrap();
        assert_eq!(val, json!(75));
    }

    // 5. set_characteristic sends correct PUT with JSON body
    #[tokio::test]
    async fn set_characteristic_sends_put() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/characteristics")
                .json_body(json!({
                    "characteristics": [{"aid": 1, "iid": 8, "value": true}]
                }));
            then.status(204);
        });
        let adapter = setup(&server);
        adapter.set_characteristic(1, 8, json!(true)).await.unwrap();
        mock.assert();
    }

    // 6. set_characteristic handles error response
    #[tokio::test]
    async fn set_characteristic_handles_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(PUT).path("/characteristics");
            then.status(500).body("internal error");
        });
        let adapter = setup(&server);
        let result = adapter.set_characteristic(1, 8, json!(true)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("set characteristic failed"));
    }

    // 7. turn_on sends correct characteristic (aid, iid=8, value=true)
    #[tokio::test]
    async fn turn_on_sends_correct_characteristic() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/characteristics")
                .json_body(json!({
                    "characteristics": [{"aid": 1, "iid": 8, "value": true}]
                }));
            then.status(204);
        });
        let adapter = setup(&server);
        adapter.turn_on("light-1").await.unwrap();
        mock.assert();
    }

    // 8. turn_off sends correct characteristic (aid, iid=8, value=false)
    #[tokio::test]
    async fn turn_off_sends_correct_characteristic() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/characteristics")
                .json_body(json!({
                    "characteristics": [{"aid": 1, "iid": 8, "value": false}]
                }));
            then.status(204);
        });
        let adapter = setup(&server);
        adapter.turn_off("light-1").await.unwrap();
        mock.assert();
    }

    // 9. set_property brightness maps to iid=9
    #[tokio::test]
    async fn set_property_brightness_maps_to_iid_9() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/characteristics")
                .json_body(json!({
                    "characteristics": [{"aid": 1, "iid": 9, "value": 80}]
                }));
            then.status(204);
        });
        let adapter = setup(&server);
        adapter.set_property("light-1", "brightness", json!(80)).await.unwrap();
        mock.assert();
    }

    // 10. set_property color_temperature maps to iid=10
    #[tokio::test]
    async fn set_property_color_temperature_maps_to_iid_10() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/characteristics")
                .json_body(json!({
                    "characteristics": [{"aid": 1, "iid": 10, "value": 350}]
                }));
            then.status(204);
        });
        let adapter = setup(&server);
        adapter.set_property("light-1", "color_temperature", json!(350)).await.unwrap();
        mock.assert();
    }

    // 11. set_property unsupported returns error
    #[tokio::test]
    async fn set_property_unsupported_returns_error() {
        let server = MockServer::start();
        let adapter = setup(&server);
        let result = adapter.set_property("light-1", "warp_drive", json!(9)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported HomeKit property"));
    }

    // 12. get_state returns JSON with aid, name, type, on fields
    #[tokio::test]
    async fn get_state_returns_correct_json() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/characteristics").query_param("id", "1.8");
            then.status(200).json_body(json!({
                "characteristics": [{"aid": 1, "iid": 8, "value": true}]
            }));
        });
        let adapter = setup(&server);
        let state = adapter.get_state("light-1").await.unwrap();
        assert_eq!(state["aid"], 1);
        assert_eq!(state["name"], "Living Room Light");
        assert_eq!(state["type"], "light");
        assert_eq!(state["on"], true);
    }

    // 13. supported_type returns "HomeKit"
    #[test]
    fn supported_type_returns_homekit() {
        let adapter = HomeKitAdapter::new(DEFAULT_BASE_URL);
        assert_eq!(adapter.supported_type(), "HomeKit");
    }

    // 14. register_accessory stores entry correctly
    #[test]
    fn register_accessory_stores_entry() {
        let mut adapter = HomeKitAdapter::new(DEFAULT_BASE_URL);
        adapter.register_accessory("fan-1", 5, "Ceiling Fan", "fan");
        let entry = adapter.get_entry("fan-1").unwrap();
        assert_eq!(entry.aid, 5);
        assert_eq!(entry.name, "Ceiling Fan");
        assert_eq!(entry.accessory_type, "fan");
    }

    // 15. unknown device_id returns error
    #[tokio::test]
    async fn unknown_device_id_returns_error() {
        let server = MockServer::start();
        let adapter = setup(&server);
        let result = adapter.turn_on("ghost-device").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown HomeKit accessory"));
    }
}
