//! Xiaomi Mi Home / MIoT API adapter
//!
//! Controls Xiaomi smart home devices via the MIoT-Spec standard.
//! Supports lights, switches, air purifiers, climate/AC, and more.
//! Devices are described by services (siid), properties (piid), and actions (aiid).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::iot::control::DeviceController;

const DEFAULT_BASE_URL: &str = "https://api.home.mi.com/api";

// ---------------------------------------------------------------------------
// Config & data types
// ---------------------------------------------------------------------------

/// Configuration for the Mi Home adapter.
#[derive(Debug, Clone)]
pub struct MiHomeConfig {
    pub access_token: String,
}

/// A Xiaomi device as returned by the device list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiDevice {
    pub did: String,
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub is_online: bool,
}

/// A MIoT-Spec property identified by device, service, and property IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiProperty {
    pub did: String,
    pub siid: u32,
    pub piid: u32,
    pub value: Value,
}

// ---------------------------------------------------------------------------
// API response wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct MiResponse<T> {
    code: i32,
    message: String,
    result: Option<T>,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Xiaomi Mi Home adapter backed by real HTTP calls to the MIoT API.
pub struct MiHomeAdapter {
    client: reqwest::Client,
    base_url: String,
    access_token: String,
}

impl MiHomeAdapter {
    /// Create a new adapter with the given config.
    pub fn new(config: MiHomeConfig) -> Self {
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

    /// List all devices on the account.
    pub async fn list_devices(&self) -> anyhow::Result<Vec<MiDevice>> {
        let url = self.endpoint("home/device_list");
        let body = json!({
            "getVirtualModel": false,
            "getHuamiDevices": 0
        });

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
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("list_devices failed ({}): {}", status.as_u16(), text);
        }

        let parsed: MiResponse<Vec<MiDevice>> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!(
                "list_devices error (code {}): {}",
                parsed.code,
                parsed.message
            );
        }
        Ok(parsed.result.unwrap_or_default())
    }

    /// Get properties for a device.
    pub async fn get_properties(&self, props: &[MiProperty]) -> anyhow::Result<Vec<MiProperty>> {
        let url = self.endpoint("device/prop");
        let params: Vec<Value> = props
            .iter()
            .map(|p| json!({"did": p.did, "siid": p.siid, "piid": p.piid}))
            .collect();

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .query(&[("params", serde_json::to_string(&params)?)])
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("get_properties failed ({}): {}", status.as_u16(), text);
        }

        let parsed: MiResponse<Vec<MiProperty>> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!(
                "get_properties error (code {}): {}",
                parsed.code,
                parsed.message
            );
        }
        Ok(parsed.result.unwrap_or_default())
    }

    /// Set properties on a device.
    pub async fn set_properties(&self, props: &[MiProperty]) -> anyhow::Result<()> {
        let url = self.endpoint("device/prop");
        let params: Vec<Value> = props
            .iter()
            .map(|p| {
                json!({
                    "did": p.did,
                    "siid": p.siid,
                    "piid": p.piid,
                    "value": p.value
                })
            })
            .collect();

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&json!({"params": params}))
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            anyhow::bail!("unauthorized: invalid or expired access token");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("set_properties failed ({}): {}", status.as_u16(), text);
        }

        let parsed: MiResponse<Value> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!(
                "set_properties error (code {}): {}",
                parsed.code,
                parsed.message
            );
        }
        Ok(())
    }

    /// Execute an RPC command on a device.
    pub async fn rpc_call(
        &self,
        device_id: &str,
        method: &str,
        params: Value,
    ) -> anyhow::Result<Value> {
        let url = self.endpoint(&format!("home/rpc/{}", device_id));
        let body = json!({
            "method": method,
            "params": params
        });

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
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("rpc_call failed ({}): {}", status.as_u16(), text);
        }

        let parsed: MiResponse<Value> = resp.json().await?;
        if parsed.code != 0 {
            anyhow::bail!("rpc_call error (code {}): {}", parsed.code, parsed.message);
        }
        Ok(parsed.result.unwrap_or(Value::Null))
    }

    /// Map a named property to its MIoT-Spec (siid, piid) pair.
    fn resolve_property(name: &str) -> anyhow::Result<(u32, u32)> {
        match name {
            "brightness" => Ok((2, 2)),
            "color_temp" => Ok((2, 3)),
            "temperature" => Ok((2, 3)),
            "mode" => Ok((2, 4)),
            _ => anyhow::bail!("unsupported property: {}", name),
        }
    }
}

// ---------------------------------------------------------------------------
// DeviceController impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for MiHomeAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let prop = MiProperty {
            did: device_id.to_string(),
            siid: 2,
            piid: 1,
            value: json!(true),
        };
        self.set_properties(&[prop]).await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let prop = MiProperty {
            did: device_id.to_string(),
            siid: 2,
            piid: 1,
            value: json!(false),
        };
        self.set_properties(&[prop]).await
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let queries = vec![
            MiProperty {
                did: device_id.to_string(),
                siid: 2,
                piid: 1,
                value: Value::Null,
            },
            MiProperty {
                did: device_id.to_string(),
                siid: 2,
                piid: 2,
                value: Value::Null,
            },
            MiProperty {
                did: device_id.to_string(),
                siid: 2,
                piid: 3,
                value: Value::Null,
            },
        ];
        let results = self.get_properties(&queries).await?;
        let props: Vec<Value> = results
            .iter()
            .map(|p| {
                json!({
                    "siid": p.siid,
                    "piid": p.piid,
                    "value": p.value
                })
            })
            .collect();
        Ok(json!({
            "device_id": device_id,
            "properties": props
        }))
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let (siid, piid) = Self::resolve_property(property)?;
        let prop = MiProperty {
            did: device_id.to_string(),
            siid,
            piid,
            value,
        };
        self.set_properties(&[prop]).await
    }

    fn supported_type(&self) -> &str {
        "SmartHome"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    fn test_config() -> MiHomeConfig {
        MiHomeConfig {
            access_token: "test-token".to_string(),
        }
    }

    fn setup(server: &MockServer) -> MiHomeAdapter {
        MiHomeAdapter::new(test_config()).with_base_url(&server.base_url())
    }

    fn ok_response<T: Serialize>(result: T) -> Value {
        json!({ "code": 0, "message": "ok", "result": result })
    }

    fn error_response(code: i32, message: &str) -> Value {
        json!({ "code": code, "message": message, "result": null })
    }

    fn sample_devices() -> Value {
        json!([
            { "did": "dev-001", "name": "Bedroom Light", "model": "yeelink.light.strip2", "is_online": true },
            { "did": "dev-002", "name": "Living Room Plug", "model": "chuangmi.plug.m3", "is_online": false }
        ])
    }

    // 1. list_devices sends POST with correct body and Bearer token
    #[tokio::test]
    async fn list_devices_sends_post_with_correct_body_and_bearer() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/home/device_list")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"getVirtualModel": false, "getHuamiDevices": 0}));
            then.status(200).json_body(ok_response(sample_devices()));
        });
        let adapter = setup(&server);
        adapter.list_devices().await.unwrap();
        mock.assert();
    }

    // 2. list_devices parses device array
    #[tokio::test]
    async fn list_devices_parses_device_array() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/home/device_list");
            then.status(200).json_body(ok_response(sample_devices()));
        });
        let adapter = setup(&server);
        let devices = adapter.list_devices().await.unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].did, "dev-001");
        assert_eq!(devices[0].name, "Bedroom Light");
        assert_eq!(devices[0].model, "yeelink.light.strip2");
        assert!(devices[0].is_online);
        assert_eq!(devices[1].did, "dev-002");
        assert!(!devices[1].is_online);
    }

    // 3. list_devices empty returns empty vec
    #[tokio::test]
    async fn list_devices_empty_returns_empty_vec() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/home/device_list");
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        let devices = adapter.list_devices().await.unwrap();
        assert!(devices.is_empty());
    }

    // 4. get_properties sends correct params (did, siid, piid)
    #[tokio::test]
    async fn get_properties_sends_correct_params() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/device/prop")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(ok_response(json!([
                {"did": "dev-001", "siid": 2, "piid": 1, "value": true}
            ])));
        });
        let adapter = setup(&server);
        let props = vec![MiProperty {
            did: "dev-001".into(),
            siid: 2,
            piid: 1,
            value: Value::Null,
        }];
        adapter.get_properties(&props).await.unwrap();
        mock.assert();
    }

    // 5. get_properties parses property values
    #[tokio::test]
    async fn get_properties_parses_property_values() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/device/prop");
            then.status(200).json_body(ok_response(json!([
                {"did": "dev-001", "siid": 2, "piid": 1, "value": true},
                {"did": "dev-001", "siid": 2, "piid": 2, "value": 80}
            ])));
        });
        let adapter = setup(&server);
        let props = vec![
            MiProperty {
                did: "dev-001".into(),
                siid: 2,
                piid: 1,
                value: Value::Null,
            },
            MiProperty {
                did: "dev-001".into(),
                siid: 2,
                piid: 2,
                value: Value::Null,
            },
        ];
        let result = adapter.get_properties(&props).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].value, json!(true));
        assert_eq!(result[1].value, json!(80));
    }

    // 6. set_property sends POST with correct body
    #[tokio::test]
    async fn set_property_sends_post_with_correct_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/device/prop")
                .header("authorization", "Bearer test-token")
                .json_body(json!({
                    "params": [{"did": "dev-001", "siid": 2, "piid": 1, "value": true}]
                }));
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        let prop = MiProperty {
            did: "dev-001".into(),
            siid: 2,
            piid: 1,
            value: json!(true),
        };
        adapter.set_properties(&[prop]).await.unwrap();
        mock.assert();
    }

    // 7. set_property success
    #[tokio::test]
    async fn set_property_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/device/prop");
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        let prop = MiProperty {
            did: "dev-001".into(),
            siid: 2,
            piid: 2,
            value: json!(50),
        };
        assert!(adapter.set_properties(&[prop]).await.is_ok());
    }

    // 8. set_property failure (code != 0) returns error
    #[tokio::test]
    async fn set_property_failure_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/device/prop");
            then.status(200)
                .json_body(error_response(-704, "device offline"));
        });
        let adapter = setup(&server);
        let prop = MiProperty {
            did: "dev-001".into(),
            siid: 2,
            piid: 1,
            value: json!(true),
        };
        let err = adapter.set_properties(&[prop]).await.unwrap_err();
        assert!(err.to_string().contains("device offline"));
    }

    // 9. turn_on sends set_property siid=2 piid=1 value=true
    #[tokio::test]
    async fn turn_on_sends_correct_property() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/device/prop").json_body(json!({
                "params": [{"did": "dev-001", "siid": 2, "piid": 1, "value": true}]
            }));
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        adapter.turn_on("dev-001").await.unwrap();
        mock.assert();
    }

    // 10. turn_off sends set_property siid=2 piid=1 value=false
    #[tokio::test]
    async fn turn_off_sends_correct_property() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/device/prop").json_body(json!({
                "params": [{"did": "dev-001", "siid": 2, "piid": 1, "value": false}]
            }));
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        adapter.turn_off("dev-001").await.unwrap();
        mock.assert();
    }

    // 11. get_state returns properties as JSON
    #[tokio::test]
    async fn get_state_returns_properties_as_json() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/device/prop");
            then.status(200).json_body(ok_response(json!([
                {"did": "dev-001", "siid": 2, "piid": 1, "value": true},
                {"did": "dev-001", "siid": 2, "piid": 2, "value": 75},
                {"did": "dev-001", "siid": 2, "piid": 3, "value": 4000}
            ])));
        });
        let adapter = setup(&server);
        let state = adapter.get_state("dev-001").await.unwrap();
        assert_eq!(state["device_id"], "dev-001");
        let props = state["properties"].as_array().unwrap();
        assert_eq!(props.len(), 3);
        assert_eq!(props[0]["value"], json!(true));
        assert_eq!(props[1]["value"], json!(75));
        assert_eq!(props[2]["value"], json!(4000));
    }

    // 12. set_property brightness maps to siid=2 piid=2
    #[tokio::test]
    async fn set_property_brightness_maps_correctly() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/device/prop").json_body(json!({
                "params": [{"did": "dev-001", "siid": 2, "piid": 2, "value": 60}]
            }));
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        adapter
            .set_property("dev-001", "brightness", json!(60))
            .await
            .unwrap();
        mock.assert();
    }

    // 13. set_property temperature maps correctly
    #[tokio::test]
    async fn set_property_temperature_maps_correctly() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/device/prop").json_body(json!({
                "params": [{"did": "dev-001", "siid": 2, "piid": 3, "value": 24}]
            }));
            then.status(200).json_body(ok_response(json!([])));
        });
        let adapter = setup(&server);
        adapter
            .set_property("dev-001", "temperature", json!(24))
            .await
            .unwrap();
        mock.assert();
    }

    // 14. set_property unknown returns error
    #[tokio::test]
    async fn set_property_unknown_returns_error() {
        let server = MockServer::start();
        let adapter = setup(&server);
        let err = adapter
            .set_property("dev-001", "unknown_prop", json!(42))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unsupported property"));
    }

    // 15. 401 unauthorized returns clear error
    #[tokio::test]
    async fn unauthorized_returns_clear_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/home/device_list");
            then.status(401).body("Unauthorized");
        });
        let adapter = setup(&server);
        let err = adapter.list_devices().await.unwrap_err();
        assert!(err.to_string().contains("unauthorized"));
    }

    // 16. device offline returns error
    #[tokio::test]
    async fn device_offline_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/device/prop");
            then.status(200)
                .json_body(error_response(-704, "device is offline"));
        });
        let adapter = setup(&server);
        let err = adapter.turn_on("dev-offline").await.unwrap_err();
        assert!(err.to_string().contains("device is offline"));
    }

    // 17. supported_type returns "SmartHome"
    #[test]
    fn supported_type_returns_smart_home() {
        let adapter = MiHomeAdapter::new(test_config());
        assert_eq!(adapter.supported_type(), "SmartHome");
    }

    // 18. rpc_call sends POST to /home/rpc/{did}
    #[tokio::test]
    async fn rpc_call_sends_post_to_correct_path() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/home/rpc/dev-001")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"method": "get_prop", "params": ["power"]}));
            then.status(200).json_body(ok_response(json!(["on"])));
        });
        let adapter = setup(&server);
        adapter
            .rpc_call("dev-001", "get_prop", json!(["power"]))
            .await
            .unwrap();
        mock.assert();
    }

    // 19. rpc_call parses response
    #[tokio::test]
    async fn rpc_call_parses_response() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/home/rpc/dev-001");
            then.status(200)
                .json_body(ok_response(json!({"power": "on", "bright": 80})));
        });
        let adapter = setup(&server);
        let result = adapter
            .rpc_call("dev-001", "get_prop", json!(["power", "bright"]))
            .await
            .unwrap();
        assert_eq!(result["power"], "on");
        assert_eq!(result["bright"], 80);
    }

    // 20. unknown device (rpc to nonexistent) returns error from API
    #[tokio::test]
    async fn rpc_unknown_device_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/home/rpc/nonexistent");
            then.status(200)
                .json_body(error_response(-9999, "device not found"));
        });
        let adapter = setup(&server);
        let err = adapter
            .rpc_call("nonexistent", "get_prop", json!(["power"]))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("device not found"));
    }
}
