//! Tuya / SmartLife OpenAPI adapter
//!
//! Controls Tuya-compatible smart-home devices via the Tuya Cloud OpenAPI.
//! Supports CN, US, and EU regions with HMAC-SHA256 request signing.

use async_trait::async_trait;
use ring::hmac;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// Tuya OpenAPI region, each with a distinct base URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TuyaRegion {
    CN,
    US,
    EU,
}

impl TuyaRegion {
    fn base_url(self) -> &'static str {
        match self {
            TuyaRegion::CN => "https://openapi.tuyacn.com",
            TuyaRegion::US => "https://openapi.tuyaus.com",
            TuyaRegion::EU => "https://openapi.tuyaeu.com",
        }
    }
}

// ---------------------------------------------------------------------------
// Config & types
// ---------------------------------------------------------------------------

/// Configuration for connecting to the Tuya OpenAPI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuyaConfig {
    pub client_id: String,
    pub client_secret: String,
    pub region: TuyaRegion,
    pub uid: String,
}
/// A single Tuya device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuyaDevice {
    pub id: String,
    pub name: String,
    pub category: String,
    pub online: bool,
}

/// A single status data-point returned by the Tuya status endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuyaStatus {
    pub code: String,
    pub value: Value,
}

/// Generic Tuya API response envelope.
#[derive(Debug, Deserialize)]
struct TuyaResponse<T> {
    success: bool,
    result: Option<T>,
    msg: Option<String>,
}

/// Token response from `/v1.0/token`.
#[derive(Debug, Deserialize)]
struct TokenResult {
    access_token: String,
}

// ---------------------------------------------------------------------------
// Signing (pure function, easy to unit-test)
// ---------------------------------------------------------------------------

/// Compute the Tuya HMAC-SHA256 signature.
///
/// `string_to_sign` = `method\nhash\n\nurl`  (hash = SHA-256 of body, empty for GET).
/// `sign = HMAC-SHA256(client_secret, client_id + t + nonce + string_to_sign)`
pub fn sign_request(
    client_id: &str,
    client_secret: &str,
    t: &str,
    nonce: &str,
    string_to_sign: &str,
) -> String {
    let message = format!("{}{}{}{}", client_id, t, nonce, string_to_sign);
    let key = hmac::Key::new(hmac::HMAC_SHA256, client_secret.as_bytes());
    let tag = hmac::sign(&key, message.as_bytes());
    tag.as_ref()
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>()
}

/// Compute SHA-256 hex digest of a byte slice (used for body hashing).
fn sha256_hex(data: &[u8]) -> String {
    use ring::digest;
    let d = digest::digest(&digest::SHA256, data);
    d.as_ref()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Tuya / SmartLife OpenAPI adapter backed by real HTTP calls.
pub struct TuyaAdapter {
    config: TuyaConfig,
    client: reqwest::Client,
    base_url: String,
    access_token: Arc<RwLock<Option<String>>>,
}

impl TuyaAdapter {
    /// Create a new adapter for the given config.
    pub fn new(config: TuyaConfig) -> Self {
        let base_url = config.region.base_url().to_string();
        Self {
            config,
            client: reqwest::Client::new(),
            base_url,
            access_token: Arc::new(RwLock::new(None)),
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

    /// Build the string-to-sign for a request.
    fn build_string_to_sign(method: &str, path: &str, body: &str) -> String {
        let body_hash = sha256_hex(body.as_bytes());
        format!("{}\n{}\n\n{}", method, body_hash, path)
    }

    /// Obtain an access token from the Tuya API.
    pub async fn get_token(&self) -> anyhow::Result<String> {
        let path = "/v1.0/token?grant_type=1";
        let url = self.endpoint(path);
        let t = chrono::Utc::now().timestamp_millis().to_string();
        let nonce = "";
        let sts = Self::build_string_to_sign("GET", path, "");
        let sign = sign_request(
            &self.config.client_id,
            &self.config.client_secret,
            &t,
            nonce,
            &sts,
        );

        let resp = self
            .client
            .get(&url)
            .header("client_id", &self.config.client_id)
            .header("sign", &sign)
            .header("t", &t)
            .header("sign_method", "HMAC-SHA256")
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("get_token HTTP failed ({}): {}", status.as_u16(), body);
        }

        let parsed: TuyaResponse<TokenResult> = resp.json().await?;
        if !parsed.success {
            anyhow::bail!(
                "get_token failed: {}",
                parsed.msg.unwrap_or_else(|| "unknown error".into())
            );
        }
        let token = parsed
            .result
            .ok_or_else(|| anyhow::anyhow!("get_token: missing result"))?
            .access_token;

        *self.access_token.write().await = Some(token.clone());
        Ok(token)
    }

    /// Ensure we have a valid access token, returning it.
    async fn ensure_token(&self) -> anyhow::Result<String> {
        {
            let guard = self.access_token.read().await;
            if let Some(ref tok) = *guard {
                return Ok(tok.clone());
            }
        }
        self.get_token().await
    }

    /// Build auth headers for an authenticated request.
    fn auth_headers(
        client_id: &str,
        client_secret: &str,
        access_token: &str,
        method: &str,
        path: &str,
        body: &str,
    ) -> Vec<(&'static str, String)> {
        let t = chrono::Utc::now().timestamp_millis().to_string();
        let nonce = "";
        let sts = Self::build_string_to_sign(method, path, body);
        let message = format!("{}{}{}{}{}", client_id, access_token, t, nonce, sts);
        let key = hmac::Key::new(hmac::HMAC_SHA256, client_secret.as_bytes());
        let tag = hmac::sign(&key, message.as_bytes());
        let sign: String = tag.as_ref().iter().map(|b| format!("{:02X}", b)).collect();

        vec![
            ("client_id", client_id.to_string()),
            ("access_token", access_token.to_string()),
            ("sign", sign),
            ("t", t),
            ("sign_method", "HMAC-SHA256".to_string()),
        ]
    }

    /// List devices for the configured user.
    pub async fn list_devices(&self) -> anyhow::Result<Vec<TuyaDevice>> {
        let token = self.ensure_token().await?;
        let path = format!("/v1.0/users/{}/devices", self.config.uid);
        let url = self.endpoint(&path);

        let headers = Self::auth_headers(
            &self.config.client_id,
            &self.config.client_secret,
            &token,
            "GET",
            &path,
            "",
        );

        let mut req = self.client.get(&url);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("token expired or unauthorized (401)");
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("list_devices failed ({}): {}", status.as_u16(), body);
        }

        let parsed: TuyaResponse<Vec<TuyaDevice>> = resp.json().await?;
        if !parsed.success {
            anyhow::bail!(
                "list_devices failed: {}",
                parsed.msg.unwrap_or_else(|| "unknown error".into())
            );
        }
        Ok(parsed.result.unwrap_or_default())
    }

    /// Get the status data-points for a device.
    pub async fn get_device_status(&self, device_id: &str) -> anyhow::Result<Vec<TuyaStatus>> {
        let token = self.ensure_token().await?;
        let path = format!("/v1.0/devices/{}/status", device_id);
        let url = self.endpoint(&path);

        let headers = Self::auth_headers(
            &self.config.client_id,
            &self.config.client_secret,
            &token,
            "GET",
            &path,
            "",
        );

        let mut req = self.client.get(&url);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("token expired or unauthorized (401)");
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("get_device_status failed ({}): {}", status.as_u16(), body);
        }

        let parsed: TuyaResponse<Vec<TuyaStatus>> = resp.json().await?;
        if !parsed.success {
            anyhow::bail!(
                "get_device_status failed: {}",
                parsed.msg.unwrap_or_else(|| "unknown error".into())
            );
        }
        Ok(parsed.result.unwrap_or_default())
    }

    /// Send commands to a device.
    pub async fn send_command(&self, device_id: &str, commands: Vec<Value>) -> anyhow::Result<()> {
        let token = self.ensure_token().await?;
        let path = format!("/v1.0/devices/{}/commands", device_id);
        let url = self.endpoint(&path);
        let body_value = json!({ "commands": commands });
        let body_str = serde_json::to_string(&body_value)?;

        let headers = Self::auth_headers(
            &self.config.client_id,
            &self.config.client_secret,
            &token,
            "POST",
            &path,
            &body_str,
        );

        let mut req = self.client.post(&url);
        for (k, v) in &headers {
            req = req.header(*k, v);
        }
        req = req.header("Content-Type", "application/json");
        req = req.body(body_str);

        let resp = req.send().await?;
        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("token expired or unauthorized (401)");
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("send_command failed ({}): {}", status.as_u16(), body);
        }

        let parsed: TuyaResponse<bool> = resp.json().await?;
        if !parsed.success {
            anyhow::bail!(
                "send_command failed: {}",
                parsed.msg.unwrap_or_else(|| "unknown error".into())
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DeviceController impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for TuyaAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, vec![json!({"code": "switch", "value": true})])
            .await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        self.send_command(device_id, vec![json!({"code": "switch", "value": false})])
            .await
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        self.send_command(device_id, vec![json!({"code": property, "value": value})])
            .await
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let statuses = self.get_device_status(device_id).await?;
        Ok(serde_json::to_value(statuses)?)
    }

    fn supported_type(&self) -> &str {
        "SmartHome"
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn test_config() -> TuyaConfig {
        TuyaConfig {
            client_id: "test_client_id".into(),
            client_secret: "test_client_secret".into(),
            region: TuyaRegion::US,
            uid: "test_uid_123".into(),
        }
    }

    fn setup(server: &MockServer) -> TuyaAdapter {
        let mut adapter = TuyaAdapter::new(test_config());
        adapter = adapter.with_base_url(&server.base_url());
        // Pre-seed a token so authenticated calls don't need to fetch one.
        let token = Arc::new(RwLock::new(Some("mock_access_token".to_string())));
        adapter.access_token = token;
        adapter
    }

    fn success_token_body() -> Value {
        json!({
            "success": true,
            "result": { "access_token": "fresh_token_abc" },
            "msg": null
        })
    }

    fn success_devices_body() -> Value {
        json!({
            "success": true,
            "result": [
                { "id": "dev001", "name": "Living Room Light", "category": "dj", "online": true },
                { "id": "dev002", "name": "Bedroom Switch", "category": "kg", "online": false }
            ],
            "msg": null
        })
    }

    fn success_status_body() -> Value {
        json!({
            "success": true,
            "result": [
                { "code": "switch", "value": true },
                { "code": "brightness", "value": 80 }
            ],
            "msg": null
        })
    }

    fn success_command_body() -> Value {
        json!({ "success": true, "result": true, "msg": null })
    }

    fn failure_body(msg: &str) -> Value {
        json!({ "success": false, "result": null, "msg": msg })
    }

    // 1. region_base_urls correct for CN/US/EU
    #[test]
    fn region_base_urls() {
        assert_eq!(TuyaRegion::CN.base_url(), "https://openapi.tuyacn.com");
        assert_eq!(TuyaRegion::US.base_url(), "https://openapi.tuyaus.com");
        assert_eq!(TuyaRegion::EU.base_url(), "https://openapi.tuyaeu.com");
    }

    // 2. get_token sends correct request, parses access_token
    #[tokio::test]
    async fn get_token_sends_correct_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/v1.0/token")
                .query_param("grant_type", "1")
                .header("client_id", "test_client_id")
                .header_exists("sign")
                .header_exists("t")
                .header("sign_method", "HMAC-SHA256");
            then.status(200).json_body(success_token_body());
        });
        let adapter = TuyaAdapter::new(test_config()).with_base_url(&server.base_url());
        let token = adapter.get_token().await.unwrap();
        assert_eq!(token, "fresh_token_abc");
        mock.assert();
    }

    // 3. get_token failure returns error
    #[tokio::test]
    async fn get_token_failure_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1.0/token");
            then.status(200)
                .json_body(failure_body("invalid client_id"));
        });
        let adapter = TuyaAdapter::new(test_config()).with_base_url(&server.base_url());
        let result = adapter.get_token().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid client_id"));
    }

    // 4. list_devices sends GET with auth headers
    #[tokio::test]
    async fn list_devices_sends_get_with_auth_headers() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/v1.0/users/test_uid_123/devices")
                .header("client_id", "test_client_id")
                .header("access_token", "mock_access_token")
                .header_exists("sign")
                .header("sign_method", "HMAC-SHA256");
            then.status(200).json_body(success_devices_body());
        });
        let adapter = setup(&server);
        adapter.list_devices().await.unwrap();
        mock.assert();
    }

    // 5. list_devices parses device array
    #[tokio::test]
    async fn list_devices_parses_device_array() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1.0/users/test_uid_123/devices");
            then.status(200).json_body(success_devices_body());
        });
        let adapter = setup(&server);
        let devices = adapter.list_devices().await.unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "dev001");
        assert_eq!(devices[0].name, "Living Room Light");
        assert_eq!(devices[0].category, "dj");
        assert!(devices[0].online);
        assert_eq!(devices[1].id, "dev002");
        assert!(!devices[1].online);
    }

    // 6. get_device_status sends correct GET
    #[tokio::test]
    async fn get_device_status_sends_correct_get() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/v1.0/devices/dev001/status")
                .header("client_id", "test_client_id")
                .header("access_token", "mock_access_token");
            then.status(200).json_body(success_status_body());
        });
        let adapter = setup(&server);
        adapter.get_device_status("dev001").await.unwrap();
        mock.assert();
    }

    // 7. get_device_status parses status array
    #[tokio::test]
    async fn get_device_status_parses_status_array() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1.0/devices/dev001/status");
            then.status(200).json_body(success_status_body());
        });
        let adapter = setup(&server);
        let statuses = adapter.get_device_status("dev001").await.unwrap();
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].code, "switch");
        assert_eq!(statuses[0].value, json!(true));
        assert_eq!(statuses[1].code, "brightness");
        assert_eq!(statuses[1].value, json!(80));
    }

    // 8. send_command sends POST with commands body
    #[tokio::test]
    async fn send_command_sends_post_with_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1.0/devices/dev001/commands")
                .header("client_id", "test_client_id")
                .header("access_token", "mock_access_token")
                .header("Content-Type", "application/json")
                .body_contains(r#""commands":[{"code":"switch","value":true}]"#);
            then.status(200).json_body(success_command_body());
        });
        let adapter = setup(&server);
        adapter
            .send_command("dev001", vec![json!({"code": "switch", "value": true})])
            .await
            .unwrap();
        mock.assert();
    }

    // 9. send_command success
    #[tokio::test]
    async fn send_command_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1.0/devices/dev001/commands");
            then.status(200).json_body(success_command_body());
        });
        let adapter = setup(&server);
        let result = adapter
            .send_command("dev001", vec![json!({"code": "switch", "value": true})])
            .await;
        assert!(result.is_ok());
    }

    // 10. send_command failure (success:false) returns error
    #[tokio::test]
    async fn send_command_failure_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1.0/devices/dev001/commands");
            then.status(200).json_body(failure_body("device offline"));
        });
        let adapter = setup(&server);
        let result = adapter
            .send_command("dev001", vec![json!({"code": "switch", "value": true})])
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("device offline"));
    }

    // 11. turn_on sends switch:true command
    #[tokio::test]
    async fn turn_on_sends_switch_true() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1.0/devices/dev001/commands")
                .body_contains(r#""code":"switch"#)
                .body_contains(r#""value":true"#);
            then.status(200).json_body(success_command_body());
        });
        let adapter = setup(&server);
        adapter.turn_on("dev001").await.unwrap();
        mock.assert();
    }

    // 12. turn_off sends switch:false command
    #[tokio::test]
    async fn turn_off_sends_switch_false() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1.0/devices/dev001/commands")
                .body_contains(r#""code":"switch"#)
                .body_contains(r#""value":false"#);
            then.status(200).json_body(success_command_body());
        });
        let adapter = setup(&server);
        adapter.turn_off("dev001").await.unwrap();
        mock.assert();
    }

    // 13. get_state returns device status as JSON
    #[tokio::test]
    async fn get_state_returns_status_json() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1.0/devices/dev001/status");
            then.status(200).json_body(success_status_body());
        });
        let adapter = setup(&server);
        let state = adapter.get_state("dev001").await.unwrap();
        let arr = state.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["code"], "switch");
        assert_eq!(arr[0]["value"], json!(true));
        assert_eq!(arr[1]["code"], "brightness");
        assert_eq!(arr[1]["value"], json!(80));
    }

    // 14. set_property sends correct command code+value
    #[tokio::test]
    async fn set_property_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1.0/devices/dev001/commands")
                .body_contains(r#""code":"brightness"#)
                .body_contains(r#""value":50"#);
            then.status(200).json_body(success_command_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("dev001", "brightness", json!(50))
            .await
            .unwrap();
        mock.assert();
    }

    // 15. sign_request produces correct HMAC-SHA256
    #[test]
    fn sign_request_produces_correct_hmac() {
        let client_id = "my_client";
        let client_secret = "my_secret";
        let t = "1234567890000";
        let nonce = "abc";
        let sts = "GET\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n\n/v1.0/token?grant_type=1";

        let sig = sign_request(client_id, client_secret, t, nonce, sts);

        // Verify independently: HMAC-SHA256(my_secret, "my_client1234567890000abc" + sts)
        let message = format!("{}{}{}{}", client_id, t, nonce, sts);
        let key = hmac::Key::new(hmac::HMAC_SHA256, client_secret.as_bytes());
        let tag = hmac::sign(&key, message.as_bytes());
        let expected: String = tag.as_ref().iter().map(|b| format!("{:02X}", b)).collect();

        assert_eq!(sig, expected);
        // Signature should be 64 hex chars (32 bytes)
        assert_eq!(sig.len(), 64);
        // All uppercase hex
        assert!(sig
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
    }

    // 16. sign_request with empty body
    #[test]
    fn sign_request_with_empty_body() {
        let sts = TuyaAdapter::build_string_to_sign("GET", "/v1.0/token?grant_type=1", "");
        // Empty body SHA-256 is the well-known hash
        assert!(sts.starts_with("GET\ne3b0c44298fc1c149afbf4c8996fb924"));
        let sig = sign_request("cid", "csec", "100", "", &sts);
        assert_eq!(sig.len(), 64);
    }

    // 17. 401 token expired returns error
    #[tokio::test]
    async fn token_expired_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1.0/devices/dev001/status");
            then.status(401).body("token expired");
        });
        let adapter = setup(&server);
        let result = adapter.get_device_status("dev001").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("401"));
    }

    // 18. unknown device returns error (API returns success:false)
    #[tokio::test]
    async fn unknown_device_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1.0/devices/nonexistent/status");
            then.status(200).json_body(failure_body("device not found"));
        });
        let adapter = setup(&server);
        let result = adapter.get_device_status("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("device not found"));
    }

    // 19. supported_type returns "SmartHome"
    #[test]
    fn supported_type_returns_smart_home() {
        let adapter = TuyaAdapter::new(test_config());
        assert_eq!(adapter.supported_type(), "SmartHome");
    }

    // 20. turn_on for unknown device (API error) propagates error
    #[tokio::test]
    async fn turn_on_unknown_device_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1.0/devices/ghost/commands");
            then.status(200).json_body(failure_body("device not found"));
        });
        let adapter = setup(&server);
        let result = adapter.turn_on("ghost").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("device not found"));
    }

    // 20b. turn_off for unknown device (API error) propagates error
    #[tokio::test]
    async fn turn_off_unknown_device_errors() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1.0/devices/ghost/commands");
            then.status(200).json_body(failure_body("device not found"));
        });
        let adapter = setup(&server);
        let result = adapter.turn_off("ghost").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("device not found"));
    }

    // Bonus: with_base_url trims trailing slash
    #[test]
    fn with_base_url_trims_trailing_slash() {
        let adapter = TuyaAdapter::new(test_config()).with_base_url("http://localhost:1234/");
        assert_eq!(adapter.base_url, "http://localhost:1234");
    }

    // Bonus: endpoint builds correct URL
    #[test]
    fn endpoint_builds_correct_url() {
        let adapter = TuyaAdapter::new(test_config()).with_base_url("http://localhost:5555");
        let url = adapter.endpoint("/v1.0/devices/dev001/status");
        assert_eq!(url, "http://localhost:5555/v1.0/devices/dev001/status");
    }
}
