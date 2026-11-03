//! Smart TV adapter
//!
//! Supports Roku (via ECP) and LG WebOS smart TV platforms.
//! Capabilities: PowerControl, InputSwitch, AppLaunch, VolumeControl

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// TV platform type
#[derive(Debug, Clone, PartialEq)]
pub enum TvType {
    Roku,
    LgWebOs,
}

/// Smart TV configuration
#[derive(Debug, Clone)]
pub struct SmartTvConfig {
    pub tv_type: TvType,
    pub host: String,
    pub api_key: Option<String>, // For LG WebOS if needed
}

// ---------------------------------------------------------------------------
// Roku ECP helpers
// ---------------------------------------------------------------------------

/// Roku ECP key names
#[allow(dead_code)]
const ROKU_KEYS: &[&str] = &[
    "Home",
    "Rev",
    "Fwd",
    "Play",
    "Select",
    "Left",
    "Right",
    "Up",
    "Down",
    "Back",
    "VolumeUp",
    "VolumeDown",
    "VolumeMute",
    "PowerOff",
    "PowerOn",
];

/// Extract text between simple XML tags
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = xml.find(&open) {
        let content_start = start + open.len();
        if let Some(end) = xml[content_start..].find(&close) {
            return Some(xml[content_start..content_start + end].to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Smart TV adapter with real Roku ECP and LG WebOS API support
pub struct SmartTvAdapter {
    tvs: HashMap<String, SmartTvConfig>,
    client: reqwest::Client,
    base_url_override: Option<String>,
}

impl SmartTvAdapter {
    pub fn new() -> Self {
        Self {
            tvs: HashMap::new(),
            client: reqwest::Client::new(),
            base_url_override: None,
        }
    }

    /// Override base URL for testing with httpmock
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url_override = Some(url.trim_end_matches('/').to_string());
        self
    }

    pub fn register_tv(&mut self, device_id: &str, config: SmartTvConfig) {
        self.tvs.insert(device_id.to_string(), config);
    }

    fn get_config(&self, device_id: &str) -> anyhow::Result<&SmartTvConfig> {
        self.tvs
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown TV: {}", device_id))
    }

    fn build_url(&self, config: &SmartTvConfig, path: &str) -> String {
        if let Some(base) = &self.base_url_override {
            format!("{}{}", base, path)
        } else {
            match config.tv_type {
                TvType::Roku => format!("http://{}:8060{}", config.host, path),
                TvType::LgWebOs => format!("http://{}{}", config.host, path),
            }
        }
    }

    // Roku ECP methods
    pub async fn send_key(&self, device_id: &str, key: &str) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;
        if config.tv_type != TvType::Roku {
            anyhow::bail!("send_key only supported on Roku");
        }

        let url = self.build_url(config, &format!("/keypress/{}", key));
        let response = self.client.post(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Roku keypress failed: {}", response.status());
        }
        Ok(())
    }

    pub async fn get_device_info(&self, device_id: &str) -> anyhow::Result<Value> {
        let config = self.get_config(device_id)?;
        if config.tv_type != TvType::Roku {
            anyhow::bail!("get_device_info only supported on Roku");
        }

        let url = self.build_url(config, "/query/device-info");
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Roku device-info failed: {}", response.status());
        }

        let xml = response.text().await?;
        let model = extract_xml_value(&xml, "model-name").unwrap_or_default();
        let serial = extract_xml_value(&xml, "serial-number").unwrap_or_default();
        let power_mode = extract_xml_value(&xml, "power-mode").unwrap_or_default();

        Ok(serde_json::json!({
            "model": model,
            "serial": serial,
            "power_mode": power_mode,
        }))
    }

    pub async fn get_active_app(&self, device_id: &str) -> anyhow::Result<Value> {
        let config = self.get_config(device_id)?;
        if config.tv_type != TvType::Roku {
            anyhow::bail!("get_active_app only supported on Roku");
        }

        let url = self.build_url(config, "/query/active-app");
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Roku active-app failed: {}", response.status());
        }

        let xml = response.text().await?;
        let app_id = extract_xml_value(&xml, "app").unwrap_or_default();

        Ok(serde_json::json!({
            "app_id": app_id,
        }))
    }

    pub async fn launch_app(&self, device_id: &str, app_id: &str) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;

        match config.tv_type {
            TvType::Roku => {
                let url = self.build_url(config, &format!("/launch/{}", app_id));
                let response = self.client.post(&url).send().await?;
                if !response.status().is_success() {
                    anyhow::bail!("Roku launch failed: {}", response.status());
                }
                Ok(())
            }
            TvType::LgWebOs => {
                let url = self.build_url(config, "/api/command");
                let payload = serde_json::json!({
                    "type": "request",
                    "uri": "ssap://system.launcher/launch",
                    "payload": {
                        "id": app_id
                    }
                });
                let response = self.client.post(&url).json(&payload).send().await?;
                if !response.status().is_success() {
                    anyhow::bail!("LG WebOS launch failed: {}", response.status());
                }
                Ok(())
            }
        }
    }

    pub async fn set_volume(&self, device_id: &str, level: u64) -> anyhow::Result<()> {
        if level > 100 {
            anyhow::bail!("volume must be 0-100");
        }

        let config = self.get_config(device_id)?;

        match config.tv_type {
            TvType::Roku => {
                // Roku doesn't support direct volume setting via ECP, only VolumeUp/Down
                anyhow::bail!("Roku does not support direct volume setting via ECP");
            }
            TvType::LgWebOs => {
                let url = self.build_url(config, "/api/command");
                let payload = serde_json::json!({
                    "type": "request",
                    "uri": "ssap://audio/setVolume",
                    "payload": {
                        "volume": level
                    }
                });
                let response = self.client.post(&url).json(&payload).send().await?;
                if !response.status().is_success() {
                    anyhow::bail!("LG WebOS set volume failed: {}", response.status());
                }
                Ok(())
            }
        }
    }

    pub async fn set_input(&self, device_id: &str, input_id: &str) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;

        match config.tv_type {
            TvType::Roku => {
                // Roku uses app launch for input switching (e.g., tvinput.hdmi1)
                let url = self.build_url(config, &format!("/launch/{}", input_id));
                let response = self.client.post(&url).send().await?;
                if !response.status().is_success() {
                    anyhow::bail!("Roku input switch failed: {}", response.status());
                }
                Ok(())
            }
            TvType::LgWebOs => {
                let url = self.build_url(config, "/api/command");
                let payload = serde_json::json!({
                    "type": "request",
                    "uri": "ssap://tv/switchInput",
                    "payload": {
                        "inputId": input_id
                    }
                });
                let response = self.client.post(&url).json(&payload).send().await?;
                if !response.status().is_success() {
                    anyhow::bail!("LG WebOS input switch failed: {}", response.status());
                }
                Ok(())
            }
        }
    }
}

#[async_trait]
impl DeviceController for SmartTvAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;

        match config.tv_type {
            TvType::Roku => self.send_key(device_id, "PowerOn").await,
            TvType::LgWebOs => {
                let url = self.build_url(config, "/api/command");
                let payload = serde_json::json!({
                    "type": "request",
                    "uri": "ssap://system/turnOn"
                });
                let response = self.client.post(&url).json(&payload).send().await?;
                if !response.status().is_success() {
                    anyhow::bail!("LG WebOS turn on failed: {}", response.status());
                }
                Ok(())
            }
        }
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;

        match config.tv_type {
            TvType::Roku => self.send_key(device_id, "PowerOff").await,
            TvType::LgWebOs => {
                let url = self.build_url(config, "/api/command");
                let payload = serde_json::json!({
                    "type": "request",
                    "uri": "ssap://system/turnOff"
                });
                let response = self.client.post(&url).json(&payload).send().await?;
                if !response.status().is_success() {
                    anyhow::bail!("LG WebOS turn off failed: {}", response.status());
                }
                Ok(())
            }
        }
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        match property {
            "volume" => {
                let level = value
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("volume must be a number"))?;
                self.set_volume(device_id, level).await
            }
            "input" => {
                let input_id = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("input must be a string"))?;
                self.set_input(device_id, input_id).await
            }
            "app" => {
                let app_id = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("app must be a string"))?;
                self.launch_app(device_id, app_id).await
            }
            "key" => {
                let key = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("key must be a string"))?;
                self.send_key(device_id, key).await
            }
            _ => anyhow::bail!("unsupported property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let config = self.get_config(device_id)?;

        match config.tv_type {
            TvType::Roku => self.get_device_info(device_id).await,
            TvType::LgWebOs => {
                // For LG WebOS, return basic info
                Ok(serde_json::json!({
                    "tv_type": "LgWebOs",
                    "host": config.host,
                }))
            }
        }
    }

    fn supported_type(&self) -> &str {
        "SmartTv"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn roku_adapter(server: &MockServer) -> SmartTvAdapter {
        let mut adapter = SmartTvAdapter::new().with_base_url(&server.base_url());
        adapter.register_tv(
            "roku-001",
            SmartTvConfig {
                tv_type: TvType::Roku,
                host: "192.168.1.100".to_string(),
                api_key: None,
            },
        );
        adapter
    }

    fn lg_adapter(server: &MockServer) -> SmartTvAdapter {
        let mut adapter = SmartTvAdapter::new().with_base_url(&server.base_url());
        adapter.register_tv(
            "lg-001",
            SmartTvConfig {
                tv_type: TvType::LgWebOs,
                host: "192.168.1.101".to_string(),
                api_key: Some("test-key".to_string()),
            },
        );
        adapter
    }

    // 1. supported_type returns SmartTv
    #[test]
    fn supported_type_is_smart_tv() {
        assert_eq!(SmartTvAdapter::new().supported_type(), "SmartTv");
    }

    // 2. Roku send_key success
    #[tokio::test]
    async fn roku_send_key_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/keypress/Home");
            then.status(200);
        });
        let adapter = roku_adapter(&server);
        adapter.send_key("roku-001", "Home").await.unwrap();
    }

    // 3. Roku send_key failure
    #[tokio::test]
    async fn roku_send_key_failure() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/keypress/Invalid");
            then.status(404);
        });
        let adapter = roku_adapter(&server);
        assert!(adapter.send_key("roku-001", "Invalid").await.is_err());
    }

    // 4. Roku get_device_info success
    #[tokio::test]
    async fn roku_get_device_info_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/query/device-info");
            then.status(200).body(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <device-info>
                    <model-name>Roku Ultra</model-name>
                    <serial-number>ABC123</serial-number>
                    <power-mode>PowerOn</power-mode>
                </device-info>"#,
            );
        });
        let adapter = roku_adapter(&server);
        let info = adapter.get_device_info("roku-001").await.unwrap();
        assert_eq!(info["model"], "Roku Ultra");
        assert_eq!(info["serial"], "ABC123");
        assert_eq!(info["power_mode"], "PowerOn");
    }

    // 5. Roku get_active_app success
    #[tokio::test]
    async fn roku_get_active_app_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/query/active-app");
            then.status(200).body(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <active-app>
                    <app>12</app>
                </active-app>"#,
            );
        });
        let adapter = roku_adapter(&server);
        let app = adapter.get_active_app("roku-001").await.unwrap();
        assert_eq!(app["app_id"], "12");
    }

    // 6. Roku launch_app success
    #[tokio::test]
    async fn roku_launch_app_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/launch/12");
            then.status(200);
        });
        let adapter = roku_adapter(&server);
        adapter.launch_app("roku-001", "12").await.unwrap();
    }

    // 7. Roku set_volume not supported
    #[tokio::test]
    async fn roku_set_volume_not_supported() {
        let server = MockServer::start();
        let adapter = roku_adapter(&server);
        assert!(adapter.set_volume("roku-001", 50).await.is_err());
    }

    // 8. Roku set_input success
    #[tokio::test]
    async fn roku_set_input_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/launch/tvinput.hdmi1");
            then.status(200);
        });
        let adapter = roku_adapter(&server);
        adapter
            .set_input("roku-001", "tvinput.hdmi1")
            .await
            .unwrap();
    }

    // 9. Roku turn_on success
    #[tokio::test]
    async fn roku_turn_on_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/keypress/PowerOn");
            then.status(200);
        });
        let adapter = roku_adapter(&server);
        adapter.turn_on("roku-001").await.unwrap();
    }

    // 10. Roku turn_off success
    #[tokio::test]
    async fn roku_turn_off_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/keypress/PowerOff");
            then.status(200);
        });
        let adapter = roku_adapter(&server);
        adapter.turn_off("roku-001").await.unwrap();
    }

    // 11. LG launch_app success
    #[tokio::test]
    async fn lg_launch_app_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/command")
                .json_body_includes(r#"{"uri":"ssap://system.launcher/launch"}"#);
            then.status(200);
        });
        let adapter = lg_adapter(&server);
        adapter.launch_app("lg-001", "netflix").await.unwrap();
    }

    // 12. LG set_volume success
    #[tokio::test]
    async fn lg_set_volume_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/command")
                .json_body_includes(r#"{"uri":"ssap://audio/setVolume"}"#);
            then.status(200);
        });
        let adapter = lg_adapter(&server);
        adapter.set_volume("lg-001", 25).await.unwrap();
    }

    // 13. LG set_volume out of range
    #[tokio::test]
    async fn lg_set_volume_out_of_range() {
        let server = MockServer::start();
        let adapter = lg_adapter(&server);
        assert!(adapter.set_volume("lg-001", 150).await.is_err());
    }

    // 14. LG set_input success
    #[tokio::test]
    async fn lg_set_input_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/command")
                .json_body_includes(r#"{"uri":"ssap://tv/switchInput"}"#);
            then.status(200);
        });
        let adapter = lg_adapter(&server);
        adapter.set_input("lg-001", "HDMI_1").await.unwrap();
    }

    // 15. LG turn_on success
    #[tokio::test]
    async fn lg_turn_on_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/command")
                .json_body_includes(r#"{"uri":"ssap://system/turnOn"}"#);
            then.status(200);
        });
        let adapter = lg_adapter(&server);
        adapter.turn_on("lg-001").await.unwrap();
    }

    // 16. LG turn_off success
    #[tokio::test]
    async fn lg_turn_off_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/command")
                .json_body_includes(r#"{"uri":"ssap://system/turnOff"}"#);
            then.status(200);
        });
        let adapter = lg_adapter(&server);
        adapter.turn_off("lg-001").await.unwrap();
    }

    // 17. set_property routes to correct methods
    #[tokio::test]
    async fn set_property_routes_correctly() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/command");
            then.status(200);
        });
        let adapter = lg_adapter(&server);

        adapter
            .set_property("lg-001", "volume", serde_json::json!(50))
            .await
            .unwrap();
        adapter
            .set_property("lg-001", "input", serde_json::json!("HDMI_2"))
            .await
            .unwrap();
        adapter
            .set_property("lg-001", "app", serde_json::json!("youtube"))
            .await
            .unwrap();
    }

    // 18. get_state returns device info
    #[tokio::test]
    async fn get_state_returns_info() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/query/device-info");
            then.status(200).body(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <device-info>
                    <model-name>Roku Express</model-name>
                    <serial-number>XYZ789</serial-number>
                    <power-mode>PowerOn</power-mode>
                </device-info>"#,
            );
        });
        let adapter = roku_adapter(&server);
        let state = adapter.get_state("roku-001").await.unwrap();
        assert_eq!(state["model"], "Roku Express");
    }

    // 19. unknown device errors
    #[tokio::test]
    async fn unknown_device_errors() {
        let server = MockServer::start();
        let adapter = roku_adapter(&server);
        assert!(adapter.get_state("unknown").await.is_err());
        assert!(adapter.turn_on("unknown").await.is_err());
        assert!(adapter.turn_off("unknown").await.is_err());
    }

    // 20. unsupported property errors
    #[tokio::test]
    async fn unsupported_property_errors() {
        let server = MockServer::start();
        let adapter = roku_adapter(&server);
        assert!(adapter
            .set_property("roku-001", "invalid", Value::Null)
            .await
            .is_err());
    }
}
