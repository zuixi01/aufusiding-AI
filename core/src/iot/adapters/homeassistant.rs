//! HomeAssistant REST API adapter
//!
//! Controls smart home devices via the HomeAssistant REST API.
//! Supports lights, switches, climate, media players, covers, fans, vacuums, and water heaters.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// HomeAssistant connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeAssistantConfig {
    pub host: String,
    pub port: u16,
    pub access_token: String,
}

// ---------------------------------------------------------------------------
// Entity state
// ---------------------------------------------------------------------------

/// A single entity state returned by the HA REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityState {
    pub entity_id: String,
    pub state: String,
    #[serde(default)]
    pub attributes: Value,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// HomeAssistant REST API adapter backed by real HTTP calls.
pub struct HomeAssistantAdapter {
    client: reqwest::Client,
    base_url: String,
    access_token: String,
}

impl HomeAssistantAdapter {
    /// Create a new adapter from config.
    pub fn new(config: HomeAssistantConfig) -> Self {
        let base_url = format!("http://{}:{}/api", config.host, config.port);
        Self {
            client: reqwest::Client::new(),
            base_url,
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

    /// GET /api/ -- health check. Returns the HA API status message.
    pub async fn health_check(&self) -> anyhow::Result<Value> {
        let url = format!("{}/", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("health_check request failed: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read health_check error body: {}", e);
                String::new()
            });
            anyhow::bail!(
                "GET /api/ health check failed ({}): {}",
                status.as_u16(),
                body
            );
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("health_check: failed to parse response: {}", e))?;
        Ok(body)
    }

    /// GET /api/states — list all entity states.
    pub async fn get_states(&self) -> anyhow::Result<Vec<EntityState>> {
        let url = self.endpoint("states");
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read get_states error body: {}", e);
                String::new()
            });
            anyhow::bail!("GET /states failed ({}): {}", status.as_u16(), body);
        }
        let entities: Vec<EntityState> = resp.json().await?;
        Ok(entities)
    }

    /// GET /api/states/{entity_id} — get a single entity state.
    pub async fn get_entity_state(&self, entity_id: &str) -> anyhow::Result<EntityState> {
        let url = self.endpoint(&format!("states/{}", entity_id));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read get_entity_state error body: {}", e);
                String::new()
            });
            anyhow::bail!(
                "GET /states/{} failed ({}): {}",
                entity_id,
                status.as_u16(),
                body
            );
        }
        let entity: EntityState = resp.json().await?;
        Ok(entity)
    }

    /// POST /api/services/{domain}/{service} — call a service on an entity.
    pub async fn call_service(
        &self,
        domain: &str,
        service: &str,
        entity_id: &str,
        data: Option<Value>,
    ) -> anyhow::Result<()> {
        let url = self.endpoint(&format!("services/{}/{}", domain, service));
        let mut body = match data {
            Some(Value::Object(map)) => Value::Object(map),
            Some(_) => json!({}),
            None => json!({}),
        };
        body.as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("call_service: body is not a JSON object"))?
            .insert(
                "entity_id".to_string(),
                Value::String(entity_id.to_string()),
            );
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let resp_body = resp.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read call_service error body: {}", e);
                String::new()
            });
            anyhow::bail!(
                "POST /services/{}/{} failed ({}): {}",
                domain,
                service,
                status.as_u16(),
                resp_body
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Domain helpers
// ---------------------------------------------------------------------------

/// Extract the domain prefix from an entity_id (e.g. "light.living_room" -> "light").
fn parse_domain(entity_id: &str) -> anyhow::Result<&str> {
    entity_id
        .split('.')
        .next()
        .filter(|d| !d.is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid entity_id format: {}", entity_id))
}

/// Map turn_on to the correct service name for special domains.
fn service_for_turn_on(domain: &str) -> &str {
    match domain {
        "vacuum" => "start",
        "cover" => "open_cover",
        _ => "turn_on",
    }
}

/// Map turn_off to the correct service name for special domains.
fn service_for_turn_off(domain: &str) -> &str {
    match domain {
        "vacuum" => "return_to_base",
        "cover" => "close_cover",
        _ => "turn_off",
    }
}

// ---------------------------------------------------------------------------
// DeviceController trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for HomeAssistantAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let domain = parse_domain(device_id)?;
        let service = service_for_turn_on(domain);
        self.call_service(domain, service, device_id, None).await
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let domain = parse_domain(device_id)?;
        let service = service_for_turn_off(domain);
        self.call_service(domain, service, device_id, None).await
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let domain = parse_domain(device_id)?;
        let (svc_domain, service, data) = match property {
            "brightness" => ("light", "turn_on", json!({ "brightness": value })),
            "temperature" => (
                "climate",
                "set_temperature",
                json!({ "temperature": value }),
            ),
            "volume" => (
                "media_player",
                "volume_set",
                json!({ "volume_level": value }),
            ),
            "hvac_mode" => ("climate", "set_hvac_mode", json!({ "hvac_mode": value })),
            "percentage" => ("fan", "set_percentage", json!({ "percentage": value })),
            _ => anyhow::bail!("unsupported property: {}", property),
        };
        // Validate domain matches the expected service domain
        if domain != svc_domain {
            anyhow::bail!(
                "property '{}' is not valid for domain '{}'",
                property,
                domain
            );
        }
        self.call_service(domain, service, device_id, Some(data))
            .await
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let entity = self.get_entity_state(device_id).await?;
        Ok(serde_json::to_value(entity)?)
    }

    fn supported_type(&self) -> &str {
        "HomeAssistant"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn config() -> HomeAssistantConfig {
        HomeAssistantConfig {
            host: "127.0.0.1".to_string(),
            port: 8123,
            access_token: "test-token-abc".to_string(),
        }
    }

    fn adapter(server: &MockServer) -> HomeAssistantAdapter {
        HomeAssistantAdapter::new(config()).with_base_url(&server.base_url())
    }

    // 1. get_states sends GET /api/states with Bearer token
    #[tokio::test]
    async fn get_states_sends_bearer_token() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/states")
                .header("Authorization", "Bearer test-token-abc");
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.get_states().await.unwrap();
        mock.assert();
    }

    // 2. get_states parses array of entities
    #[tokio::test]
    async fn get_states_parses_entities() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/states");
            then.status(200).json_body(json!([
                {"entity_id": "light.kitchen", "state": "on", "attributes": {"brightness": 200}},
                {"entity_id": "switch.garage", "state": "off", "attributes": {}}
            ]));
        });
        let ha = adapter(&server);
        let states = ha.get_states().await.unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].entity_id, "light.kitchen");
        assert_eq!(states[0].state, "on");
        assert_eq!(states[1].entity_id, "switch.garage");
    }

    // 3. get_state sends GET /api/states/{entity_id}
    #[tokio::test]
    async fn get_entity_state_sends_correct_path() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/states/light.bedroom");
            then.status(200).json_body(json!({
                "entity_id": "light.bedroom",
                "state": "off",
                "attributes": {}
            }));
        });
        let ha = adapter(&server);
        ha.get_entity_state("light.bedroom").await.unwrap();
        mock.assert();
    }

    // 4. get_state parses single entity
    #[tokio::test]
    async fn get_entity_state_parses_entity() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/states/climate.living");
            then.status(200).json_body(json!({
                "entity_id": "climate.living",
                "state": "heat",
                "attributes": {"temperature": 22}
            }));
        });
        let ha = adapter(&server);
        let entity = ha.get_entity_state("climate.living").await.unwrap();
        assert_eq!(entity.entity_id, "climate.living");
        assert_eq!(entity.state, "heat");
        assert_eq!(entity.attributes["temperature"], 22);
    }

    // 5. call_service sends POST with entity_id in body
    #[tokio::test]
    async fn call_service_sends_post_with_entity_id() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/light/turn_on")
                .header("Authorization", "Bearer test-token-abc")
                .json_body(json!({"entity_id": "light.kitchen"}));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.call_service("light", "turn_on", "light.kitchen", None)
            .await
            .unwrap();
        mock.assert();
    }

    // 6. call_service with extra data merges into body
    #[tokio::test]
    async fn call_service_merges_extra_data() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/light/turn_on")
                .json_body(json!({
                    "entity_id": "light.kitchen",
                    "brightness": 128
                }));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.call_service(
            "light",
            "turn_on",
            "light.kitchen",
            Some(json!({"brightness": 128})),
        )
        .await
        .unwrap();
        mock.assert();
    }

    // 7. turn_on light extracts domain correctly
    #[tokio::test]
    async fn turn_on_light_extracts_domain() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/light/turn_on")
                .json_body(json!({"entity_id": "light.living_room"}));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.turn_on("light.living_room").await.unwrap();
        mock.assert();
    }

    // 8. turn_off switch extracts domain correctly
    #[tokio::test]
    async fn turn_off_switch_extracts_domain() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/switch/turn_off")
                .json_body(json!({"entity_id": "switch.garage"}));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.turn_off("switch.garage").await.unwrap();
        mock.assert();
    }

    // 9. turn_on climate calls climate/turn_on
    #[tokio::test]
    async fn turn_on_climate_calls_correct_service() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/climate/turn_on")
                .json_body(json!({"entity_id": "climate.bedroom"}));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.turn_on("climate.bedroom").await.unwrap();
        mock.assert();
    }

    // 10. get_state returns entity as JSON
    #[tokio::test]
    async fn get_state_returns_json() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/states/light.desk");
            then.status(200).json_body(json!({
                "entity_id": "light.desk",
                "state": "on",
                "attributes": {"brightness": 255}
            }));
        });
        let ha = adapter(&server);
        let val = ha.get_state("light.desk").await.unwrap();
        assert_eq!(val["entity_id"], "light.desk");
        assert_eq!(val["state"], "on");
        assert_eq!(val["attributes"]["brightness"], 255);
    }

    // 11. set_property brightness calls light/turn_on with brightness data
    #[tokio::test]
    async fn set_property_brightness() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/light/turn_on")
                .json_body(json!({
                    "entity_id": "light.kitchen",
                    "brightness": 180
                }));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.set_property("light.kitchen", "brightness", json!(180))
            .await
            .unwrap();
        mock.assert();
    }

    // 12. set_property temperature calls climate/set_temperature
    #[tokio::test]
    async fn set_property_temperature() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/climate/set_temperature")
                .json_body(json!({
                    "entity_id": "climate.living",
                    "temperature": 23
                }));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.set_property("climate.living", "temperature", json!(23))
            .await
            .unwrap();
        mock.assert();
    }

    // 13. set_property volume calls media_player/volume_set
    #[tokio::test]
    async fn set_property_volume() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/media_player/volume_set")
                .json_body(json!({
                    "entity_id": "media_player.speaker",
                    "volume_level": 0.5
                }));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.set_property("media_player.speaker", "volume", json!(0.5))
            .await
            .unwrap();
        mock.assert();
    }

    // 14. set_property hvac_mode calls climate/set_hvac_mode
    #[tokio::test]
    async fn set_property_hvac_mode() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/climate/set_hvac_mode")
                .json_body(json!({
                    "entity_id": "climate.office",
                    "hvac_mode": "cool"
                }));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.set_property("climate.office", "hvac_mode", json!("cool"))
            .await
            .unwrap();
        mock.assert();
    }

    // 15. set_property unknown returns error
    #[tokio::test]
    async fn set_property_unknown_returns_error() {
        let server = MockServer::start();
        let ha = adapter(&server);
        let result = ha
            .set_property("light.kitchen", "color_rgb", json!([255, 0, 0]))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsupported property"));
    }

    // 16. 401 unauthorized returns clear error
    #[tokio::test]
    async fn unauthorized_returns_clear_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/states");
            then.status(401).body("401: Unauthorized");
        });
        let ha = adapter(&server);
        let err = ha.get_states().await.unwrap_err();
        assert!(err.to_string().contains("401"));
    }

    // 17. 404 entity not found returns clear error
    #[tokio::test]
    async fn not_found_returns_clear_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/states/light.nonexistent");
            then.status(404).body("Entity not found");
        });
        let ha = adapter(&server);
        let err = ha.get_entity_state("light.nonexistent").await.unwrap_err();
        assert!(err.to_string().contains("404"));
    }

    // 18. 500 server error returns clear error
    #[tokio::test]
    async fn server_error_returns_clear_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/services/light/turn_on");
            then.status(500).body("Internal Server Error");
        });
        let ha = adapter(&server);
        let err = ha
            .call_service("light", "turn_on", "light.kitchen", None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("500"));
    }

    // 19. supported_type returns "HomeAssistant"
    #[test]
    fn supported_type_returns_homeassistant() {
        let ha = HomeAssistantAdapter::new(config());
        assert_eq!(ha.supported_type(), "HomeAssistant");
    }

    // 20. turn_on vacuum calls vacuum/start
    #[tokio::test]
    async fn turn_on_vacuum_calls_start() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/vacuum/start")
                .json_body(json!({"entity_id": "vacuum.roborock"}));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.turn_on("vacuum.roborock").await.unwrap();
        mock.assert();
    }

    // 21. turn_off vacuum calls vacuum/return_to_base
    #[tokio::test]
    async fn turn_off_vacuum_calls_return_to_base() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/vacuum/return_to_base")
                .json_body(json!({"entity_id": "vacuum.roborock"}));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.turn_off("vacuum.roborock").await.unwrap();
        mock.assert();
    }

    // 22. turn_on cover calls cover/open_cover
    #[tokio::test]
    async fn turn_on_cover_calls_open_cover() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/cover/open_cover")
                .json_body(json!({"entity_id": "cover.garage_door"}));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.turn_on("cover.garage_door").await.unwrap();
        mock.assert();
    }

    // 23. turn_off cover calls cover/close_cover
    #[tokio::test]
    async fn turn_off_cover_calls_close_cover() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/cover/close_cover")
                .json_body(json!({"entity_id": "cover.garage_door"}));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.turn_off("cover.garage_door").await.unwrap();
        mock.assert();
    }

    // 24. set_property fan percentage calls fan/set_percentage
    #[tokio::test]
    async fn set_property_fan_percentage() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/fan/set_percentage")
                .json_body(json!({
                    "entity_id": "fan.ceiling",
                    "percentage": 75
                }));
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        ha.set_property("fan.ceiling", "percentage", json!(75))
            .await
            .unwrap();
        mock.assert();
    }

    // 25. get_states empty array returns empty vec
    #[tokio::test]
    async fn get_states_empty_returns_empty_vec() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/states");
            then.status(200).json_body(json!([]));
        });
        let ha = adapter(&server);
        let states = ha.get_states().await.unwrap();
        assert!(states.is_empty());
    }

    // 26. health_check returns API status
    #[tokio::test]
    async fn health_check_returns_status() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/")
                .header("Authorization", "Bearer test-token-abc");
            then.status(200)
                .json_body(json!({"message": "API running."}));
        });
        let ha = adapter(&server);
        let result = ha.health_check().await.unwrap();
        assert_eq!(result["message"], "API running.");
        mock.assert();
    }

    // 27. health_check returns error on failure
    #[tokio::test]
    async fn health_check_returns_error_on_failure() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/");
            then.status(503).body("Service Unavailable");
        });
        let ha = adapter(&server);
        let err = ha.health_check().await.unwrap_err();
        assert!(err.to_string().contains("503"));
    }
}
