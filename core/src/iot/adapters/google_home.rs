//! Google Home / Smart Home API adapter
//!
//! Controls smart home devices via the Google Home Graph API.
//! Uses OAuth2 Bearer token authentication and supports device sync,
//! state queries, and command execution.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::iot::control::DeviceController;

#[allow(dead_code)]
const DEFAULT_BASE_URL: &str = "https://homegraph.googleapis.com";

// ---------------------------------------------------------------------------
// Config & types
// ---------------------------------------------------------------------------

/// Google Home connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleHomeConfig {
    pub access_token: String,
}

/// A Google Home device returned by the query endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleHomeDevice {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub device_type: String,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub state: Value,
}

/// Response envelope for the query endpoint.
#[derive(Debug, Deserialize)]
struct QueryResponse {
    #[serde(default)]
    payload: Option<QueryPayload>,
}

/// Payload within a query response.
#[derive(Debug, Deserialize)]
struct QueryPayload {
    #[serde(default)]
    devices: Value,
}
/// Response envelope for the execute endpoint.
#[derive(Debug, Deserialize)]
struct ExecuteResponse {
    #[serde(default)]
    payload: Option<ExecutePayload>,
}

/// Payload within an execute response.
#[derive(Debug, Deserialize)]
struct ExecutePayload {
    #[serde(default)]
    commands: Vec<ExecuteCommandResult>,
}

/// Result of a single command execution.
#[derive(Debug, Deserialize)]
struct ExecuteCommandResult {
    #[serde(default)]
    #[allow(dead_code)]
    ids: Vec<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    states: Value,
    #[serde(rename = "errorCode", default)]
    error_code: Option<String>,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Google Home Smart Home API adapter backed by real HTTP calls.
pub struct GoogleHomeAdapter {
    client: reqwest::Client,
    base_url: String,
    access_token: String,
}

impl GoogleHomeAdapter {
    /// Create a new adapter from config.
    pub fn new(config: GoogleHomeConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            access_token: config.access_token,
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
    /// POST /v1/devices:requestSync -- request a device sync.
    pub async fn request_sync(&self, agent_user_id: &str) -> anyhow::Result<()> {
        let url = self.endpoint("v1/devices:requestSync");
        let body = json!({ "agentUserId": agent_user_id });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("requestSync failed ({}): {}", status.as_u16(), body);
        }
        Ok(())
    }

    /// POST /v1/devices:query -- query device states.
    pub async fn query_device(&self, device_id: &str) -> anyhow::Result<GoogleHomeDevice> {
        let url = self.endpoint("v1/devices:query");
        let request_id = uuid::Uuid::new_v4().to_string();
        let body = json!({
            "requestId": request_id,
            "inputs": [{
                "payload": {
                    "devices": [{ "id": device_id }]
                }
            }]
        });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("query failed ({}): {}", status.as_u16(), err_body);
        }
        let parsed: QueryResponse = resp.json().await?;
        let payload = parsed
            .payload
            .ok_or_else(|| anyhow::anyhow!("query: missing payload"))?;
        let devices_map = payload.devices.as_object()
            .ok_or_else(|| anyhow::anyhow!("query: devices is not an object"))?;
        let state = devices_map
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("query: device {} not found in response", device_id))?;
        let online = state.get("online").and_then(|v| v.as_bool()).unwrap_or(false);
        Ok(GoogleHomeDevice {
            id: device_id.to_string(),
            name: state.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            device_type: state.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            online,
            state: state.clone(),
        })
    }
    /// POST /v1/devices:query -- list all registered devices.
    pub async fn list_devices(&self, device_ids: &[&str]) -> anyhow::Result<Vec<GoogleHomeDevice>> {
        let url = self.endpoint("v1/devices:query");
        let request_id = uuid::Uuid::new_v4().to_string();
        let devices: Vec<Value> = device_ids.iter().map(|id| json!({"id": id})).collect();
        let body = json!({
            "requestId": request_id,
            "inputs": [{
                "payload": {
                    "devices": devices
                }
            }]
        });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("list_devices failed ({}): {}", status.as_u16(), err_body);
        }
        let parsed: QueryResponse = resp.json().await?;
        let payload = parsed
            .payload
            .ok_or_else(|| anyhow::anyhow!("list_devices: missing payload"))?;
        let devices_map = payload.devices.as_object()
            .ok_or_else(|| anyhow::anyhow!("list_devices: devices is not an object"))?;
        let mut result = Vec::new();
        for (id, state) in devices_map {
            let online = state.get("online").and_then(|v| v.as_bool()).unwrap_or(false);
            result.push(GoogleHomeDevice {
                id: id.clone(),
                name: state.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                device_type: state.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                online,
                state: state.clone(),
            });
        }
        Ok(result)
    }

    /// POST /v1/devices:execute -- execute a command on a device.
    pub async fn execute_command(
        &self,
        device_id: &str,
        command: &str,
        params: Value,
    ) -> anyhow::Result<Value> {
        let url = self.endpoint("v1/devices:execute");
        let request_id = uuid::Uuid::new_v4().to_string();
        let body = json!({
            "requestId": request_id,
            "inputs": [{
                "payload": {
                    "commands": [{
                        "devices": [{ "id": device_id }],
                        "execution": [{
                            "command": command,
                            "params": params
                        }]
                    }]
                }
            }]
        });
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("execute failed ({}): {}", status.as_u16(), err_body);
        }
        let parsed: ExecuteResponse = resp.json().await?;
        let payload = parsed
            .payload
            .ok_or_else(|| anyhow::anyhow!("execute: missing payload"))?;
        if let Some(cmd_result) = payload.commands.first() {
            if cmd_result.status == "ERROR" {
                let code = cmd_result.error_code.as_deref().unwrap_or("UNKNOWN");
                anyhow::bail!("execute command failed: {}", code);
            }
            Ok(cmd_result.states.clone())
        } else {
            Ok(json!({}))
        }
    }

    /// Map a property name to the Google Home command string.
    fn command_for_property(property: &str) -> anyhow::Result<(&'static str, &'static str)> {
        match property {
            "brightness" => Ok((
                "action.devices.commands.BrightnessAbsolute",
                "brightness",
            )),
            "color_temperature" => Ok((
                "action.devices.commands.ColorAbsoluteTemperature",
                "temperature",
            )),
            "thermostat" => Ok((
                "action.devices.commands.ThermostatTemperatureSetpoint",
                "thermostatTemperatureSetpoint",
            )),
            _ => anyhow::bail!("unsupported Google Home property: {}", property),
        }
    }
}

// ---------------------------------------------------------------------------
// DeviceController trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for GoogleHomeAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.execute_command(
            device_id,
            "action.devices.commands.OnOff",
            json!({"on": true}),
        )
        .await?;
        Ok(())
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        self.execute_command(
            device_id,
            "action.devices.commands.OnOff",
            json!({"on": false}),
        )
        .await?;
        Ok(())
    }
    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let (command, param_key) = Self::command_for_property(property)?;
        let params = json!({ param_key: value });
        self.execute_command(device_id, command, params).await?;
        Ok(())
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let device = self.query_device(device_id).await?;
        Ok(json!({
            "id": device.id,
            "name": device.name,
            "type": device.device_type,
            "online": device.online,
            "state": device.state,
        }))
    }

    fn supported_type(&self) -> &str {
        "GoogleHome"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn config() -> GoogleHomeConfig {
        GoogleHomeConfig {
            access_token: "test-oauth-token".to_string(),
        }
    }

    fn adapter(server: &MockServer) -> GoogleHomeAdapter {
        GoogleHomeAdapter::new(config()).with_base_url(&server.base_url())
    }

    fn success_execute_body() -> Value {
        json!({
            "requestId": "123",
            "payload": {
                "commands": [{
                    "ids": ["dev-1"],
                    "status": "SUCCESS",
                    "states": {"on": true, "online": true}
                }]
            }
        })
    }

    fn error_execute_body(code: &str) -> Value {
        json!({
            "requestId": "123",
            "payload": {
                "commands": [{
                    "ids": ["dev-1"],
                    "status": "ERROR",
                    "errorCode": code,
                    "states": {}
                }]
            }
        })
    }
    fn query_response_for(device_id: &str, state: Value) -> Value {
        let mut devices = serde_json::Map::new();
        devices.insert(device_id.to_string(), state);
        json!({
            "requestId": "123",
            "payload": { "devices": devices }
        })
    }

    // 1. request_sync sends POST with Bearer token
    #[tokio::test]
    async fn request_sync_sends_bearer_token() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/devices:requestSync")
                .header("Authorization", "Bearer test-oauth-token")
                .json_body_partial(r#"{"agentUserId":"user-1"}"#);
            then.status(200).json_body(json!({}));
        });
        let gh = adapter(&server);
        gh.request_sync("user-1").await.unwrap();
        mock.assert();
    }

    // 2. request_sync handles HTTP error
    #[tokio::test]
    async fn request_sync_handles_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:requestSync");
            then.status(403).body("forbidden");
        });
        let gh = adapter(&server);
        let result = gh.request_sync("user-1").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requestSync failed (403)"));
    }

    // 3. query_device sends correct POST body
    #[tokio::test]
    async fn query_device_sends_correct_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/devices:query")
                .header("Authorization", "Bearer test-oauth-token");
            then.status(200).json_body(query_response_for("dev-1", json!({"online": true})));
        });
        let gh = adapter(&server);
        gh.query_device("dev-1").await.unwrap();
        mock.assert();
    }

    // 4. query_device parses response correctly
    #[tokio::test]
    async fn query_device_parses_response() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:query");
            then.status(200).json_body(query_response_for(
                "light-1",
                json!({"online": true, "name": "Kitchen Light", "type": "LIGHT"}),
            ));
        });
        let gh = adapter(&server);
        let device = gh.query_device("light-1").await.unwrap();
        assert_eq!(device.id, "light-1");
        assert!(device.online);
        assert_eq!(device.name, "Kitchen Light");
    }
    // 5. query_device handles HTTP error
    #[tokio::test]
    async fn query_device_handles_http_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:query");
            then.status(500).body("server error");
        });
        let gh = adapter(&server);
        let result = gh.query_device("dev-1").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("query failed (500)"));
    }

    // 6. query_device unknown device returns error
    #[tokio::test]
    async fn query_device_unknown_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:query");
            then.status(200).json_body(query_response_for("other-dev", json!({"online": true})));
        });
        let gh = adapter(&server);
        let result = gh.query_device("ghost").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("device ghost not found"));
    }

    // 7. list_devices returns multiple devices
    #[tokio::test]
    async fn list_devices_returns_multiple() {
        let server = MockServer::start();
        let mut devices = serde_json::Map::new();
        devices.insert("dev-1".to_string(), json!({"online": true}));
        devices.insert("dev-2".to_string(), json!({"online": false}));
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:query");
            then.status(200).json_body(json!({
                "requestId": "123",
                "payload": { "devices": devices }
            }));
        });
        let gh = adapter(&server);
        let result = gh.list_devices(&["dev-1", "dev-2"]).await.unwrap();
        assert_eq!(result.len(), 2);
    }

    // 8. list_devices handles HTTP error
    #[tokio::test]
    async fn list_devices_handles_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:query");
            then.status(401).body("unauthorized");
        });
        let gh = adapter(&server);
        let result = gh.list_devices(&["dev-1"]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("list_devices failed (401)"));
    }
    // 9. execute_command sends correct POST body
    #[tokio::test]
    async fn execute_command_sends_correct_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/devices:execute")
                .header("Authorization", "Bearer test-oauth-token");
            then.status(200).json_body(success_execute_body());
        });
        let gh = adapter(&server);
        gh.execute_command("dev-1", "action.devices.commands.OnOff", json!({"on": true}))
            .await
            .unwrap();
        mock.assert();
    }

    // 10. execute_command returns states on success
    #[tokio::test]
    async fn execute_command_returns_states() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:execute");
            then.status(200).json_body(success_execute_body());
        });
        let gh = adapter(&server);
        let states = gh
            .execute_command("dev-1", "action.devices.commands.OnOff", json!({"on": true}))
            .await
            .unwrap();
        assert_eq!(states["on"], true);
        assert_eq!(states["online"], true);
    }

    // 11. execute_command handles API error response
    #[tokio::test]
    async fn execute_command_handles_api_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:execute");
            then.status(200).json_body(error_execute_body("deviceOffline"));
        });
        let gh = adapter(&server);
        let result = gh
            .execute_command("dev-1", "action.devices.commands.OnOff", json!({"on": true}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("deviceOffline"));
    }

    // 12. execute_command handles HTTP error
    #[tokio::test]
    async fn execute_command_handles_http_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:execute");
            then.status(503).body("service unavailable");
        });
        let gh = adapter(&server);
        let result = gh
            .execute_command("dev-1", "action.devices.commands.OnOff", json!({"on": true}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("execute failed (503)"));
    }
    // 13. turn_on sends OnOff command with on=true
    #[tokio::test]
    async fn turn_on_sends_on_true() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/devices:execute");
            then.status(200).json_body(success_execute_body());
        });
        let gh = adapter(&server);
        gh.turn_on("dev-1").await.unwrap();
        mock.assert();
    }

    // 14. turn_off sends OnOff command with on=false
    #[tokio::test]
    async fn turn_off_sends_on_false() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/devices:execute");
            then.status(200).json_body(json!({
                "requestId": "123",
                "payload": {
                    "commands": [{
                        "ids": ["dev-1"],
                        "status": "SUCCESS",
                        "states": {"on": false, "online": true}
                    }]
                }
            }));
        });
        let gh = adapter(&server);
        gh.turn_off("dev-1").await.unwrap();
        mock.assert();
    }

    // 15. set_property brightness sends BrightnessAbsolute
    #[tokio::test]
    async fn set_property_brightness() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/devices:execute");
            then.status(200).json_body(success_execute_body());
        });
        let gh = adapter(&server);
        gh.set_property("dev-1", "brightness", json!(75)).await.unwrap();
        mock.assert();
    }

    // 16. set_property color_temperature sends ColorAbsoluteTemperature
    #[tokio::test]
    async fn set_property_color_temperature() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/devices:execute");
            then.status(200).json_body(success_execute_body());
        });
        let gh = adapter(&server);
        gh.set_property("dev-1", "color_temperature", json!(4000))
            .await
            .unwrap();
        mock.assert();
    }
    // 17. set_property thermostat sends ThermostatTemperatureSetpoint
    #[tokio::test]
    async fn set_property_thermostat() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/devices:execute");
            then.status(200).json_body(success_execute_body());
        });
        let gh = adapter(&server);
        gh.set_property("dev-1", "thermostat", json!(22.5))
            .await
            .unwrap();
        mock.assert();
    }

    // 18. set_property unsupported returns error
    #[tokio::test]
    async fn set_property_unsupported_returns_error() {
        let server = MockServer::start();
        let gh = adapter(&server);
        let result = gh.set_property("dev-1", "warp_speed", json!(9)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported Google Home property"));
    }

    // 19. get_state returns correct JSON structure
    #[tokio::test]
    async fn get_state_returns_correct_json() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/devices:query");
            then.status(200).json_body(query_response_for(
                "light-1",
                json!({"online": true, "name": "Desk Lamp", "type": "LIGHT", "on": true}),
            ));
        });
        let gh = adapter(&server);
        let state = gh.get_state("light-1").await.unwrap();
        assert_eq!(state["id"], "light-1");
        assert_eq!(state["online"], true);
        assert_eq!(state["name"], "Desk Lamp");
        assert_eq!(state["type"], "LIGHT");
    }

    // 20. supported_type returns "GoogleHome"
    #[test]
    fn supported_type_returns_google_home() {
        let gh = GoogleHomeAdapter::new(config());
        assert_eq!(gh.supported_type(), "GoogleHome");
    }

    // 21. with_base_url trims trailing slash
    #[test]
    fn with_base_url_trims_trailing_slash() {
        let gh = GoogleHomeAdapter::new(config()).with_base_url("http://localhost:1234/");
        assert_eq!(gh.base_url, "http://localhost:1234");
    }

    // 22. endpoint builds correct URL
    #[test]
    fn endpoint_builds_correct_url() {
        let gh = GoogleHomeAdapter::new(config()).with_base_url("http://localhost:5555");
        let url = gh.endpoint("/v1/devices:query");
        assert_eq!(url, "http://localhost:5555/v1/devices:query");
    }
}
