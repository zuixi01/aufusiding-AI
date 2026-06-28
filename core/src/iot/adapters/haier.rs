//! Haier U+ / UHomeOS API adapter
//!
//! Controls Haier smart home devices via the Haier U+ cloud API.
//! Supports air conditioners, water heaters, washing machines, and refrigerators.
//! Devices are controlled via named commands (cmdName) with argument maps (cmdArgs).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::iot::control::DeviceController;

const DEFAULT_BASE_URL: &str = "https://uws.haier.net";

// ---------------------------------------------------------------------------
// Config & data types
// ---------------------------------------------------------------------------

/// Configuration for the Haier U+ adapter.
#[derive(Debug, Clone)]
pub struct HaierConfig {
    pub client_id: String,
    pub client_secret: String,
    pub access_token: String,
}

/// A Haier device as returned by the device list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaierDevice {
    pub id: String,
    pub name: String,
    #[serde(rename = "deviceType", default)]
    pub device_type: String,
    #[serde(default)]
    pub online: bool,
}

/// A device attribute key-value pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaierAttribute {
    pub name: String,
    pub value: Value,
}

// ---------------------------------------------------------------------------
// API response wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct HaierResponse<T> {
    #[serde(rename = "retCode")]
    ret_code: String,
    #[serde(rename = "retInfo")]
    ret_info: String,
    data: Option<T>,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Haier U+ adapter backed by real HTTP calls to the UHomeOS API.
pub struct HaierAdapter {
    client: reqwest::Client,
    base_url: String,
    access_token: String,
    client_id: String,
    devices: HashMap<String, HaierDevice>,
}

impl HaierAdapter {
    /// Create a new adapter with the given config.
    pub fn new(config: HaierConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            access_token: config.access_token,
            client_id: config.client_id,
            devices: HashMap::new(),
        }
    }

    /// Override the base URL (for testing with httpmock).
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// GET /uds/v1/protected/deviceinfos -- list all devices on the account.
    pub async fn list_devices(&self) -> anyhow::Result<Vec<HaierDevice>> {
        let url = self.endpoint("uds/v1/protected/deviceinfos");

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header("clientId", &self.client_id)
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

        let parsed: HaierResponse<Vec<HaierDevice>> = resp.json().await?;
        if parsed.ret_code != "00000" {
            anyhow::bail!(
                "list_devices error ({}): {}",
                parsed.ret_code,
                parsed.ret_info
            );
        }
        Ok(parsed.data.unwrap_or_default())
    }

    /// GET /uds/v1/protected/{device_id}/attributes -- get device attributes.
    pub async fn get_attributes(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Vec<HaierAttribute>> {
        let url = self.endpoint(&format!(
            "uds/v1/protected/{}/attributes",
            device_id
        ));

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header("clientId", &self.client_id)
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "get_attributes failed ({}): {}",
                status.as_u16(),
                text
            );
        }

        let parsed: HaierResponse<Vec<HaierAttribute>> = resp.json().await?;
        if parsed.ret_code != "00000" {
            anyhow::bail!(
                "get_attributes error ({}): {}",
                parsed.ret_code,
                parsed.ret_info
            );
        }
        Ok(parsed.data.unwrap_or_default())
    }

    /// POST /uds/v1/protected/{device_id}/commands -- send a command.
    pub async fn send_command(
        &self,
        device_id: &str,
        cmd_name: &str,
        cmd_args: Value,
    ) -> anyhow::Result<()> {
        let url = self.endpoint(&format!(
            "uds/v1/protected/{}/commands",
            device_id
        ));
        let body = json!({
            "cmdName": cmd_name,
            "cmdArgs": cmd_args
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .header("clientId", &self.client_id)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "send_command failed ({}): {}",
                status.as_u16(),
                text
            );
        }

        let parsed: HaierResponse<Value> = resp.json().await?;
        if parsed.ret_code != "00000" {
            anyhow::bail!(
                "send_command error ({}): {}",
                parsed.ret_code,
                parsed.ret_info
            );
        }
        Ok(())
    }

    /// Cache a device locally after listing.
    pub fn cache_device(&mut self, device: HaierDevice) {
        self.devices.insert(device.id.clone(), device);
    }

    /// Look up a cached device by ID.
    pub fn get_cached_device(&self, device_id: &str) -> Option<&HaierDevice> {
        self.devices.get(device_id)
    }
}

// ---------------------------------------------------------------------------
// DeviceController impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for HaierAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(
            device_id,
            "grSetDAC",
            json!({"onOffStatus": "true"}),
        )
        .await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(
            device_id,
            "grSetDAC",
            json!({"onOffStatus": "false"}),
        )
        .await
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let cmd_args = match property {
            "temperature" => json!({"targetTemp": value.to_string().trim_matches('"')}),
            "mode" => json!({"operationMode": value.to_string().trim_matches('"')}),
            _ => anyhow::bail!("unsupported property: {}", property),
        };
        self.send_command(device_id, "grSetDAC", cmd_args).await
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let attrs = self.get_attributes(device_id).await?;
        let attr_map: HashMap<String, Value> = attrs
            .into_iter()
            .map(|a| (a.name, a.value))
            .collect();
        Ok(json!({
            "device_id": device_id,
            "attributes": attr_map
        }))
    }

    fn supported_type(&self) -> &str {
        "HaierSmartHome"
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    fn test_config() -> HaierConfig {
        HaierConfig {
            client_id: "test-client-id".to_string(),
            client_secret: "test-client-secret".to_string(),
            access_token: "test-access-token".to_string(),
        }
    }

    fn setup(server: &MockServer) -> HaierAdapter {
        HaierAdapter::new(test_config()).with_base_url(&server.base_url())
    }

    fn ok_response<T: Serialize>(data: T) -> Value {
        json!({
            "retCode": "00000",
            "retInfo": "success",
            "data": data
        })
    }

    fn error_response(code: &str, info: &str) -> Value {
        json!({
            "retCode": code,
            "retInfo": info,
            "data": null
        })
    }

    fn sample_devices() -> Value {
        json!([
            {
                "id": "dev-ac-001",
                "name": "Living Room AC",
                "deviceType": "air_conditioner",
                "online": true
            },
            {
                "id": "dev-wh-002",
                "name": "Bathroom Water Heater",
                "deviceType": "water_heater",
                "online": false
            }
        ])
    }

    // 1. list_devices sends correct headers
    #[tokio::test]
    async fn list_devices_sends_correct_headers() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/uds/v1/protected/deviceinfos")
                .header("Authorization", "Bearer test-access-token")
                .header("clientId", "test-client-id");
            then.status(200).json_body(ok_response(sample_devices()));
        });
        let adapter = setup(&server);
        adapter.list_devices().await.unwrap();
        mock.assert();
    }

    // 2. list_devices parses device array
    #[tokio::test]
    async fn list_devices_parses_devices() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/uds/v1/protected/deviceinfos");
            then.status(200).json_body(ok_response(sample_devices()));
        });
        let adapter = setup(&server);
        let devices = adapter.list_devices().await.unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "dev-ac-001");
        assert_eq!(devices[0].device_type, "air_conditioner");
        assert!(devices[0].online);
        assert_eq!(devices[1].id, "dev-wh-002");
        assert!(!devices[1].online);
    }

    // 3. list_devices returns empty on empty array
    #[tokio::test]
    async fn list_devices_empty_returns_empty_vec() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/uds/v1/protected/deviceinfos");
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        let devices = adapter.list_devices().await.unwrap();
        assert!(devices.is_empty());
    }

    // 4. list_devices 401 returns unauthorized error
    #[tokio::test]
    async fn list_devices_401_returns_unauthorized() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/uds/v1/protected/deviceinfos");
            then.status(401).body("Unauthorized");
        });
        let adapter = setup(&server);
        let err = adapter.list_devices().await.unwrap_err();
        assert!(err.to_string().contains("unauthorized"));
    }

    // 5. list_devices API error propagates
    #[tokio::test]
    async fn list_devices_api_error_propagates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/uds/v1/protected/deviceinfos");
            then.status(200)
                .json_body(error_response("A0001", "token expired"));
        });
        let adapter = setup(&server);
        let err = adapter.list_devices().await.unwrap_err();
        assert!(err.to_string().contains("token expired"));
    }

    // 6. get_attributes sends correct path
    #[tokio::test]
    async fn get_attributes_sends_correct_path() {
        let server = MockServer::start();
        let attrs = json!([
            {"name": "onOffStatus", "value": "true"},
            {"name": "targetTemp", "value": "24"}
        ]);
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/uds/v1/protected/dev-ac-001/attributes")
                .header("Authorization", "Bearer test-access-token");
            then.status(200).json_body(ok_response(attrs));
        });
        let adapter = setup(&server);
        adapter.get_attributes("dev-ac-001").await.unwrap();
        mock.assert();
    }

    // 7. get_attributes parses attributes
    #[tokio::test]
    async fn get_attributes_parses_attributes() {
        let server = MockServer::start();
        let attrs = json!([
            {"name": "onOffStatus", "value": "true"},
            {"name": "targetTemp", "value": "24"}
        ]);
        server.mock(|when, then| {
            when.method(GET)
                .path("/uds/v1/protected/dev-ac-001/attributes");
            then.status(200).json_body(ok_response(attrs));
        });
        let adapter = setup(&server);
        let result = adapter.get_attributes("dev-ac-001").await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "onOffStatus");
        assert_eq!(result[1].name, "targetTemp");
    }

    // 8. get_attributes 401 returns unauthorized
    #[tokio::test]
    async fn get_attributes_401_returns_unauthorized() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/uds/v1/protected/dev-ac-001/attributes");
            then.status(401).body("Unauthorized");
        });
        let adapter = setup(&server);
        let err = adapter.get_attributes("dev-ac-001").await.unwrap_err();
        assert!(err.to_string().contains("unauthorized"));
    }

    // 9. send_command sends correct body
    #[tokio::test]
    async fn send_command_sends_correct_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/uds/v1/protected/dev-ac-001/commands")
                .header("Authorization", "Bearer test-access-token")
                .header("clientId", "test-client-id")
                .json_body(json!({
                    "cmdName": "grSetDAC",
                    "cmdArgs": {"onOffStatus": "true"}
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter
            .send_command("dev-ac-001", "grSetDAC", json!({"onOffStatus": "true"}))
            .await
            .unwrap();
        mock.assert();
    }

    // 10. send_command API error propagates
    #[tokio::test]
    async fn send_command_api_error_propagates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/uds/v1/protected/dev-ac-001/commands");
            then.status(200)
                .json_body(error_response("E0001", "device offline"));
        });
        let adapter = setup(&server);
        let err = adapter
            .send_command("dev-ac-001", "grSetDAC", json!({"onOffStatus": "true"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("device offline"));
    }

    // 11. turn_on sends grSetDAC with onOffStatus true
    #[tokio::test]
    async fn turn_on_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/uds/v1/protected/dev-ac-001/commands")
                .json_body(json!({
                    "cmdName": "grSetDAC",
                    "cmdArgs": {"onOffStatus": "true"}
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter.turn_on("dev-ac-001").await.unwrap();
        mock.assert();
    }

    // 12. turn_off sends grSetDAC with onOffStatus false
    #[tokio::test]
    async fn turn_off_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/uds/v1/protected/dev-ac-001/commands")
                .json_body(json!({
                    "cmdName": "grSetDAC",
                    "cmdArgs": {"onOffStatus": "false"}
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter.turn_off("dev-ac-001").await.unwrap();
        mock.assert();
    }

    // 13. set_property temperature sends grSetDAC with targetTemp
    #[tokio::test]
    async fn set_property_temperature() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/uds/v1/protected/dev-ac-001/commands")
                .json_body(json!({
                    "cmdName": "grSetDAC",
                    "cmdArgs": {"targetTemp": "24"}
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter
            .set_property("dev-ac-001", "temperature", json!(24))
            .await
            .unwrap();
        mock.assert();
    }

    // 14. set_property mode sends grSetDAC with operationMode
    #[tokio::test]
    async fn set_property_mode() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/uds/v1/protected/dev-ac-001/commands")
                .json_body(json!({
                    "cmdName": "grSetDAC",
                    "cmdArgs": {"operationMode": "cool"}
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter
            .set_property("dev-ac-001", "mode", json!("cool"))
            .await
            .unwrap();
        mock.assert();
    }

    // 15. set_property unsupported property returns error
    #[tokio::test]
    async fn set_property_unsupported_returns_error() {
        let server = MockServer::start();
        let adapter = setup(&server);
        let err = adapter
            .set_property("dev-ac-001", "brightness", json!(50))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unsupported property"));
    }

    // 16. get_state fetches attributes and returns JSON
    #[tokio::test]
    async fn get_state_returns_attributes_as_json() {
        let server = MockServer::start();
        let attrs = json!([
            {"name": "onOffStatus", "value": "true"},
            {"name": "targetTemp", "value": "26"},
            {"name": "operationMode", "value": "cool"}
        ]);
        server.mock(|when, then| {
            when.method(GET)
                .path("/uds/v1/protected/dev-ac-001/attributes");
            then.status(200).json_body(ok_response(attrs));
        });
        let adapter = setup(&server);
        let state = adapter.get_state("dev-ac-001").await.unwrap();
        assert_eq!(state["device_id"], "dev-ac-001");
        assert_eq!(state["attributes"]["onOffStatus"], "true");
        assert_eq!(state["attributes"]["targetTemp"], "26");
    }

    // 17. supported_type returns HaierSmartHome
    #[test]
    fn supported_type_returns_correct_value() {
        let adapter = HaierAdapter::new(test_config());
        assert_eq!(adapter.supported_type(), "HaierSmartHome");
    }

    // 18. with_base_url trims trailing slash
    #[test]
    fn with_base_url_trims_trailing_slash() {
        let adapter =
            HaierAdapter::new(test_config()).with_base_url("http://localhost:1234/");
        assert_eq!(adapter.base_url, "http://localhost:1234");
    }

    // 19. endpoint builds correct URL
    #[test]
    fn endpoint_builds_correct_url() {
        let adapter =
            HaierAdapter::new(test_config()).with_base_url("http://localhost:5555");
        let url = adapter.endpoint("/uds/v1/protected/deviceinfos");
        assert_eq!(url, "http://localhost:5555/uds/v1/protected/deviceinfos");
    }

    // 20. cache_device and get_cached_device work
    #[test]
    fn cache_device_stores_and_retrieves() {
        let mut adapter = HaierAdapter::new(test_config());
        let device = HaierDevice {
            id: "dev-001".to_string(),
            name: "Test AC".to_string(),
            device_type: "air_conditioner".to_string(),
            online: true,
        };
        adapter.cache_device(device);
        let cached = adapter.get_cached_device("dev-001");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().name, "Test AC");
    }

    // 21. send_command 401 returns unauthorized
    #[tokio::test]
    async fn send_command_401_returns_unauthorized() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/uds/v1/protected/dev-ac-001/commands");
            then.status(401).body("Unauthorized");
        });
        let adapter = setup(&server);
        let err = adapter
            .send_command("dev-ac-001", "grSetDAC", json!({"onOffStatus": "true"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unauthorized"));
    }

    // 22. list_devices HTTP 500 returns error
    #[tokio::test]
    async fn list_devices_500_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/uds/v1/protected/deviceinfos");
            then.status(500).body("Internal Server Error");
        });
        let adapter = setup(&server);
        let err = adapter.list_devices().await.unwrap_err();
        assert!(err.to_string().contains("list_devices failed (500)"));
    }
}
