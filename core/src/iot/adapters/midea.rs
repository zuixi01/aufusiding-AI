//! Midea MSmartHome / MIoT API adapter
//!
//! Controls Midea smart home appliances via the MSmartHome cloud API.
//! Supports air conditioners, dehumidifiers, fans, and water heaters.
//! Commands use an order/body envelope with operationType and parameters.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::iot::control::DeviceController;

const DEFAULT_BASE_URL: &str = "https://mp-prod.appsmb.com";

// ---------------------------------------------------------------------------
// Config & data types
// ---------------------------------------------------------------------------

/// Configuration for the Midea MSmartHome adapter.
#[derive(Debug, Clone)]
pub struct MideaConfig {
    pub access_token: String,
    pub app_key: String,
}

/// A Midea appliance as returned by the appliance list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MideaAppliance {
    pub id: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub appliance_type: String,
    #[serde(default)]
    pub online: bool,
}

/// Status entry for an appliance attribute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MideaStatus {
    pub key: String,
    pub value: Value,
}

// ---------------------------------------------------------------------------
// API response wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct MideaResponse<T> {
    code: i32,
    msg: String,
    data: Option<T>,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Midea MSmartHome adapter backed by real HTTP calls to the MIoT API.
pub struct MideaAdapter {
    client: reqwest::Client,
    base_url: String,
    access_token: String,
    app_key: String,
    devices: HashMap<String, MideaAppliance>,
}

impl MideaAdapter {
    /// Create a new adapter with the given config.
    pub fn new(config: MideaConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            access_token: config.access_token,
            app_key: config.app_key,
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

    /// GET /mas/v5/app/proxy?alias=/v1/appliance/home/list -- list all appliances.
    pub async fn list_appliances(&self) -> anyhow::Result<Vec<MideaAppliance>> {
        let url = self.endpoint("mas/v5/app/proxy");

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header("x-app-key", &self.app_key)
            .query(&[("alias", "/v1/appliance/home/list")])
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("list_appliances failed ({}): {}", status.as_u16(), text);
        }

        let parsed: MideaResponse<Vec<MideaAppliance>> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!(
                "list_appliances error (code {}): {}",
                parsed.code,
                parsed.msg
            );
        }
        Ok(parsed.data.unwrap_or_default())
    }

    /// GET /mas/v5/app/proxy?alias=/v1/appliance/{id}/status -- get appliance status.
    pub async fn get_status(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Vec<MideaStatus>> {
        let url = self.endpoint("mas/v5/app/proxy");
        let alias = format!("/v1/appliance/{}/status", device_id);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .header("x-app-key", &self.app_key)
            .query(&[("alias", alias.as_str())])
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("get_status failed ({}): {}", status.as_u16(), text);
        }

        let parsed: MideaResponse<Vec<MideaStatus>> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!(
                "get_status error (code {}): {}",
                parsed.code,
                parsed.msg
            );
        }
        Ok(parsed.data.unwrap_or_default())
    }

    /// POST /mas/v5/app/proxy?alias=/v1/appliance/{id}/command -- send a command.
    pub async fn send_command(
        &self,
        device_id: &str,
        operation_type: &str,
        parameters: Value,
    ) -> anyhow::Result<()> {
        let url = self.endpoint("mas/v5/app/proxy");
        let alias = format!("/v1/appliance/{}/command", device_id);
        let body = json!({
            "order": {
                "body": {
                    "operationType": operation_type,
                    "parameters": parameters
                }
            }
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .header("x-app-key", &self.app_key)
            .query(&[("alias", alias.as_str())])
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("send_command failed ({}): {}", status.as_u16(), text);
        }

        let parsed: MideaResponse<Value> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!(
                "send_command error (code {}): {}",
                parsed.code,
                parsed.msg
            );
        }
        Ok(())
    }

    /// Cache an appliance locally after listing.
    pub fn cache_appliance(&mut self, appliance: MideaAppliance) {
        self.devices.insert(appliance.id.clone(), appliance);
    }

    /// Look up a cached appliance by ID.
    pub fn get_cached_appliance(&self, device_id: &str) -> Option<&MideaAppliance> {
        self.devices.get(device_id)
    }
}

// ---------------------------------------------------------------------------
// DeviceController impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for MideaAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(
            device_id,
            "powerOn",
            json!({"powerStatus": true}),
        )
        .await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(
            device_id,
            "powerOff",
            json!({"powerStatus": false}),
        )
        .await
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let (op_type, params) = match property {
            "temperature" => (
                "setTemperature",
                json!({"targetTemperature": value}),
            ),
            "mode" => (
                "setMode",
                json!({"operationMode": value}),
            ),
            "fan_speed" => (
                "setFanSpeed",
                json!({"fanSpeed": value}),
            ),
            _ => anyhow::bail!("unsupported property: {}", property),
        };
        self.send_command(device_id, op_type, params).await
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let statuses = self.get_status(device_id).await?;
        let status_map: HashMap<String, Value> = statuses
            .into_iter()
            .map(|s| (s.key, s.value))
            .collect();
        Ok(json!({
            "device_id": device_id,
            "status": status_map
        }))
    }

    fn supported_type(&self) -> &str {
        "MideaSmartHome"
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

    fn test_config() -> MideaConfig {
        MideaConfig {
            access_token: "test-access-token".to_string(),
            app_key: "test-app-key".to_string(),
        }
    }

    fn setup(server: &MockServer) -> MideaAdapter {
        MideaAdapter::new(test_config()).with_base_url(&server.base_url())
    }

    fn ok_response<T: Serialize>(data: T) -> Value {
        json!({
            "code": 0,
            "msg": "success",
            "data": data
        })
    }

    fn error_response(code: i32, msg: &str) -> Value {
        json!({
            "code": code,
            "msg": msg,
            "data": null
        })
    }

    fn sample_appliances() -> Value {
        json!([
            {
                "id": "app-ac-001",
                "name": "Bedroom AC",
                "type": "ac",
                "online": true
            },
            {
                "id": "app-dh-002",
                "name": "Basement Dehumidifier",
                "type": "dehumidifier",
                "online": false
            }
        ])
    }

    // 1. list_appliances sends correct headers
    #[tokio::test]
    async fn list_appliances_sends_correct_headers() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/mas/v5/app/proxy")
                .query_param("alias", "/v1/appliance/home/list")
                .header("Authorization", "Bearer test-access-token")
                .header("x-app-key", "test-app-key");
            then.status(200).json_body(ok_response(sample_appliances()));
        });
        let adapter = setup(&server);
        adapter.list_appliances().await.unwrap();
        mock.assert();
    }

    // 2. list_appliances parses appliance array
    #[tokio::test]
    async fn list_appliances_parses_appliances() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/mas/v5/app/proxy");
            then.status(200).json_body(ok_response(sample_appliances()));
        });
        let adapter = setup(&server);
        let appliances = adapter.list_appliances().await.unwrap();
        assert_eq!(appliances.len(), 2);
        assert_eq!(appliances[0].id, "app-ac-001");
        assert_eq!(appliances[0].appliance_type, "ac");
        assert!(appliances[0].online);
        assert_eq!(appliances[1].id, "app-dh-002");
        assert!(!appliances[1].online);
    }

    // 3. list_appliances returns empty on empty array
    #[tokio::test]
    async fn list_appliances_empty_returns_empty_vec() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/mas/v5/app/proxy");
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        let appliances = adapter.list_appliances().await.unwrap();
        assert!(appliances.is_empty());
    }

    // 4. list_appliances 401 returns unauthorized error
    #[tokio::test]
    async fn list_appliances_401_returns_unauthorized() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/mas/v5/app/proxy");
            then.status(401).body("Unauthorized");
        });
        let adapter = setup(&server);
        let err = adapter.list_appliances().await.unwrap_err();
        assert!(err.to_string().contains("unauthorized"));
    }

    // 5. list_appliances API error propagates
    #[tokio::test]
    async fn list_appliances_api_error_propagates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/mas/v5/app/proxy");
            then.status(200)
                .json_body(error_response(-1, "invalid token"));
        });
        let adapter = setup(&server);
        let err = adapter.list_appliances().await.unwrap_err();
        assert!(err.to_string().contains("invalid token"));
    }

    // 6. get_status sends correct alias query param
    #[tokio::test]
    async fn get_status_sends_correct_alias() {
        let server = MockServer::start();
        let statuses = json!([
            {"key": "powerStatus", "value": true},
            {"key": "targetTemperature", "value": 24}
        ]);
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/mas/v5/app/proxy")
                .query_param("alias", "/v1/appliance/app-ac-001/status")
                .header("Authorization", "Bearer test-access-token");
            then.status(200).json_body(ok_response(statuses));
        });
        let adapter = setup(&server);
        adapter.get_status("app-ac-001").await.unwrap();
        mock.assert();
    }

    // 7. get_status parses status entries
    #[tokio::test]
    async fn get_status_parses_entries() {
        let server = MockServer::start();
        let statuses = json!([
            {"key": "powerStatus", "value": true},
            {"key": "targetTemperature", "value": 24}
        ]);
        server.mock(|when, then| {
            when.method(GET).path("/mas/v5/app/proxy");
            then.status(200).json_body(ok_response(statuses));
        });
        let adapter = setup(&server);
        let result = adapter.get_status("app-ac-001").await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "powerStatus");
        assert_eq!(result[1].key, "targetTemperature");
    }

    // 8. get_status 401 returns unauthorized
    #[tokio::test]
    async fn get_status_401_returns_unauthorized() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/mas/v5/app/proxy");
            then.status(401).body("Unauthorized");
        });
        let adapter = setup(&server);
        let err = adapter.get_status("app-ac-001").await.unwrap_err();
        assert!(err.to_string().contains("unauthorized"));
    }

    // 9. send_command sends correct order body
    #[tokio::test]
    async fn send_command_sends_correct_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/mas/v5/app/proxy")
                .query_param("alias", "/v1/appliance/app-ac-001/command")
                .header("Authorization", "Bearer test-access-token")
                .header("x-app-key", "test-app-key")
                .json_body(json!({
                    "order": {
                        "body": {
                            "operationType": "powerOn",
                            "parameters": {"powerStatus": true}
                        }
                    }
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter
            .send_command("app-ac-001", "powerOn", json!({"powerStatus": true}))
            .await
            .unwrap();
        mock.assert();
    }

    // 10. send_command API error propagates
    #[tokio::test]
    async fn send_command_api_error_propagates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/mas/v5/app/proxy");
            then.status(200)
                .json_body(error_response(-2, "appliance offline"));
        });
        let adapter = setup(&server);
        let err = adapter
            .send_command("app-ac-001", "powerOn", json!({"powerStatus": true}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("appliance offline"));
    }

    // 11. turn_on sends powerOn command
    #[tokio::test]
    async fn turn_on_sends_power_on() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/mas/v5/app/proxy")
                .json_body(json!({
                    "order": {
                        "body": {
                            "operationType": "powerOn",
                            "parameters": {"powerStatus": true}
                        }
                    }
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter.turn_on("app-ac-001").await.unwrap();
        mock.assert();
    }

    // 12. turn_off sends powerOff command
    #[tokio::test]
    async fn turn_off_sends_power_off() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/mas/v5/app/proxy")
                .json_body(json!({
                    "order": {
                        "body": {
                            "operationType": "powerOff",
                            "parameters": {"powerStatus": false}
                        }
                    }
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter.turn_off("app-ac-001").await.unwrap();
        mock.assert();
    }

    // 13. set_property temperature sends setTemperature
    #[tokio::test]
    async fn set_property_temperature() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/mas/v5/app/proxy")
                .json_body(json!({
                    "order": {
                        "body": {
                            "operationType": "setTemperature",
                            "parameters": {"targetTemperature": 22}
                        }
                    }
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter
            .set_property("app-ac-001", "temperature", json!(22))
            .await
            .unwrap();
        mock.assert();
    }

    // 14. set_property mode sends setMode
    #[tokio::test]
    async fn set_property_mode() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/mas/v5/app/proxy")
                .json_body(json!({
                    "order": {
                        "body": {
                            "operationType": "setMode",
                            "parameters": {"operationMode": "cool"}
                        }
                    }
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter
            .set_property("app-ac-001", "mode", json!("cool"))
            .await
            .unwrap();
        mock.assert();
    }

    // 15. set_property fan_speed sends setFanSpeed
    #[tokio::test]
    async fn set_property_fan_speed() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/mas/v5/app/proxy")
                .json_body(json!({
                    "order": {
                        "body": {
                            "operationType": "setFanSpeed",
                            "parameters": {"fanSpeed": "high"}
                        }
                    }
                }));
            then.status(200).json_body(ok_response(json!({})));
        });
        let adapter = setup(&server);
        adapter
            .set_property("app-ac-001", "fan_speed", json!("high"))
            .await
            .unwrap();
        mock.assert();
    }

    // 16. set_property unsupported property returns error
    #[tokio::test]
    async fn set_property_unsupported_returns_error() {
        let server = MockServer::start();
        let adapter = setup(&server);
        let err = adapter
            .set_property("app-ac-001", "brightness", json!(50))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unsupported property"));
    }

    // 17. get_state fetches status and returns JSON
    #[tokio::test]
    async fn get_state_returns_status_as_json() {
        let server = MockServer::start();
        let statuses = json!([
            {"key": "powerStatus", "value": true},
            {"key": "targetTemperature", "value": 26},
            {"key": "operationMode", "value": "cool"}
        ]);
        server.mock(|when, then| {
            when.method(GET).path("/mas/v5/app/proxy");
            then.status(200).json_body(ok_response(statuses));
        });
        let adapter = setup(&server);
        let state = adapter.get_state("app-ac-001").await.unwrap();
        assert_eq!(state["device_id"], "app-ac-001");
        assert_eq!(state["status"]["powerStatus"], true);
        assert_eq!(state["status"]["targetTemperature"], 26);
    }

    // 18. supported_type returns MideaSmartHome
    #[test]
    fn supported_type_returns_correct_value() {
        let adapter = MideaAdapter::new(test_config());
        assert_eq!(adapter.supported_type(), "MideaSmartHome");
    }

    // 19. with_base_url trims trailing slash
    #[test]
    fn with_base_url_trims_trailing_slash() {
        let adapter =
            MideaAdapter::new(test_config()).with_base_url("http://localhost:1234/");
        assert_eq!(adapter.base_url, "http://localhost:1234");
    }

    // 20. endpoint builds correct URL
    #[test]
    fn endpoint_builds_correct_url() {
        let adapter =
            MideaAdapter::new(test_config()).with_base_url("http://localhost:5555");
        let url = adapter.endpoint("/mas/v5/app/proxy");
        assert_eq!(url, "http://localhost:5555/mas/v5/app/proxy");
    }

    // 21. cache_appliance and get_cached_appliance work
    #[test]
    fn cache_appliance_stores_and_retrieves() {
        let mut adapter = MideaAdapter::new(test_config());
        let appliance = MideaAppliance {
            id: "app-001".to_string(),
            name: "Test AC".to_string(),
            appliance_type: "ac".to_string(),
            online: true,
        };
        adapter.cache_appliance(appliance);
        let cached = adapter.get_cached_appliance("app-001");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().name, "Test AC");
    }

    // 22. send_command 401 returns unauthorized
    #[tokio::test]
    async fn send_command_401_returns_unauthorized() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/mas/v5/app/proxy");
            then.status(401).body("Unauthorized");
        });
        let adapter = setup(&server);
        let err = adapter
            .send_command("app-ac-001", "powerOn", json!({"powerStatus": true}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unauthorized"));
    }

    // 23. list_appliances HTTP 500 returns error
    #[tokio::test]
    async fn list_appliances_500_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/mas/v5/app/proxy");
            then.status(500).body("Internal Server Error");
        });
        let adapter = setup(&server);
        let err = adapter.list_appliances().await.unwrap_err();
        assert!(err.to_string().contains("list_appliances failed (500)"));
    }
}
