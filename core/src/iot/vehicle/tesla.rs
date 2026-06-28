//! Tesla Fleet API adapter
//!
//! Real HTTP implementation for Tesla vehicle control via the Fleet API.
//! Supports wake, lock/unlock, climate, charging, honk, flash, trunk.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::iot::control::DeviceController;
use crate::iot::vehicle::oauth::TokenStore;
use crate::iot::vehicle::types::VehicleState;

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// Tesla Fleet API region, each with a distinct base URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeslaRegion {
    NorthAmerica,
    Europe,
    China,
}

impl TeslaRegion {
    fn base_url(self) -> &'static str {
        match self {
            TeslaRegion::NorthAmerica => "https://fleet-api.prd.na.vn.cloud.tesla.com",
            TeslaRegion::Europe => "https://fleet-api.prd.eu.vn.cloud.tesla.com",
            TeslaRegion::China => "https://fleet-api.prd.cn.vn.cloud.tesla.com",
        }
    }
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CommandResult {
    result: bool,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CommandResponse {
    response: CommandResult,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DriveState {
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ChargeState {
    battery_level: Option<f64>,
    battery_range: Option<f64>,
    charging_state: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct VehicleStateApi {
    locked: Option<bool>,
    odometer: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ClimateState {
    is_climate_on: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct VehicleDataInner {
    drive_state: Option<DriveState>,
    charge_state: Option<ChargeState>,
    vehicle_state: Option<VehicleStateApi>,
    climate_state: Option<ClimateState>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct VehicleDataResponse {
    response: VehicleDataInner,
}

// ---------------------------------------------------------------------------
// Vehicle entry
// ---------------------------------------------------------------------------

struct TeslaVehicleEntry {
    vehicle_id: String,
    #[allow(dead_code)]
    vin: String,
    token: TokenStore,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Tesla Fleet API adapter backed by real HTTP calls.
pub struct TeslaAdapter {
    vehicles: HashMap<String, TeslaVehicleEntry>,
    client: reqwest::Client,
    base_url: String,
}

impl TeslaAdapter {
    /// Create a new adapter for the given region.
    pub fn new(region: TeslaRegion) -> Self {
        Self {
            vehicles: HashMap::new(),
            client: reqwest::Client::new(),
            base_url: region.base_url().to_string(),
        }
    }

    /// Override the base URL (for testing with httpmock).
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    /// Register a vehicle with its own token.
    pub fn register_vehicle(
        &mut self,
        device_id: &str,
        vehicle_id: &str,
        vin: &str,
        token: TokenStore,
    ) {
        self.vehicles.insert(
            device_id.to_string(),
            TeslaVehicleEntry {
                vehicle_id: vehicle_id.to_string(),
                vin: vin.to_string(),
                token,
            },
        );
    }

    fn get_entry(&self, device_id: &str) -> anyhow::Result<&TeslaVehicleEntry> {
        self.vehicles
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown vehicle: {}", device_id))
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// Wake up the vehicle.
    async fn wake_up(&self, entry: &TeslaVehicleEntry) -> anyhow::Result<()> {
        let url = self.endpoint(&format!("api/1/vehicles/{}/wake_up", entry.vehicle_id));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&entry.token.access_token)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("wake_up failed ({}): {}", status.as_u16(), body);
        }

        let parsed: CommandResponse = resp.json().await?;
        if !parsed.response.result {
            anyhow::bail!("wake_up rejected: {}", parsed.response.reason);
        }
        Ok(())
    }

    /// Fetch full vehicle data and map to VehicleState.
    async fn vehicle_data(&self, entry: &TeslaVehicleEntry) -> anyhow::Result<VehicleState> {
        let url = self.endpoint(&format!("api/1/vehicles/{}/vehicle_data", entry.vehicle_id));

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&entry.token.access_token)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("vehicle_data failed ({}): {}", status.as_u16(), body);
        }

        let data: VehicleDataResponse = resp.json().await?;
        let inner = data.response;
        let location = inner
            .drive_state
            .as_ref()
            .and_then(|d| match (d.latitude, d.longitude) {
                (Some(lat), Some(lng)) => Some((lat, lng)),
                _ => None,
            });

        let charging = inner
            .charge_state
            .as_ref()
            .and_then(|c| c.charging_state.as_deref())
            .map(|s| s == "Charging")
            .unwrap_or(false);

        let range_km = inner
            .charge_state
            .as_ref()
            .and_then(|c| c.battery_range)
            .map(|miles| miles * 1.60934);

        let odometer_km = inner
            .vehicle_state
            .as_ref()
            .and_then(|v| v.odometer)
            .map(|miles| miles * 1.60934);

        Ok(VehicleState {
            locked: inner
                .vehicle_state
                .as_ref()
                .and_then(|v| v.locked)
                .unwrap_or(false),
            climate_on: inner
                .climate_state
                .as_ref()
                .and_then(|c| c.is_climate_on)
                .unwrap_or(false),
            charge_level_pct: inner.charge_state.as_ref().and_then(|c| c.battery_level),
            location,
            odometer_km,
            charging,
            range_km,
        })
    }

    /// Send a command (POST) with optional JSON body.
    async fn send_command(
        &self,
        entry: &TeslaVehicleEntry,
        cmd: &str,
        body: Option<Value>,
    ) -> anyhow::Result<()> {
        let url = self.endpoint(&format!(
            "api/1/vehicles/{}/command/{}",
            entry.vehicle_id, cmd
        ));

        let mut req = self
            .client
            .post(&url)
            .bearer_auth(&entry.token.access_token);

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req.send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("command {} failed ({}): {}", cmd, status.as_u16(), body);
        }

        let parsed: CommandResponse = resp.json().await?;
        if !parsed.response.result {
            anyhow::bail!("command {} rejected: {}", cmd, parsed.response.reason);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DeviceController impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for TeslaAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let entry = self.get_entry(device_id)?;
        self.wake_up(entry).await
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("Tesla API does not support remote shutdown")
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let entry = self.get_entry(device_id)?;
        match property {
            "lock" => {
                let lock = value.as_bool().unwrap_or(true);
                let cmd = if lock { "door_lock" } else { "door_unlock" };
                self.send_command(entry, cmd, None).await
            }
            "climate" => {
                let on = value.as_bool().unwrap_or(false);
                let cmd = if on {
                    "auto_conditioning_start"
                } else {
                    "auto_conditioning_stop"
                };
                self.send_command(entry, cmd, None).await
            }
            "charge_limit" => {
                let pct = value.as_u64().unwrap_or(80);
                self.send_command(entry, "set_charge_limit", Some(json!({ "percent": pct })))
                    .await
            }
            "honk" => self.send_command(entry, "honk_horn", None).await,
            "flash" => self.send_command(entry, "flash_lights", None).await,
            "trunk" => {
                self.send_command(
                    entry,
                    "actuate_trunk",
                    Some(json!({ "which_trunk": "rear" })),
                )
                .await
            }
            _ => anyhow::bail!("unsupported Tesla property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let entry = self.get_entry(device_id)?;
        let state = self.vehicle_data(entry).await?;
        Ok(serde_json::to_value(state)?)
    }

    fn supported_type(&self) -> &str {
        "Vehicle"
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn test_token() -> TokenStore {
        TokenStore {
            access_token: "test-token".into(),
            refresh_token: None,
            expires_at: None,
            token_type: "Bearer".into(),
            scope: None,
        }
    }

    fn success_body() -> Value {
        json!({"response": {"result": true, "reason": ""}})
    }

    fn failure_body(reason: &str) -> Value {
        json!({"response": {"result": false, "reason": reason}})
    }

    fn vehicle_data_body() -> Value {
        json!({
            "response": {
                "drive_state": {"latitude": 37.38, "longitude": -122.08},
                "charge_state": {
                    "battery_level": 78,
                    "battery_range": 192.5,
                    "charging_state": "Disconnected"
                },
                "vehicle_state": {"locked": true, "odometer": 15534.2},
                "climate_state": {"is_climate_on": false}
            }
        })
    }

    fn setup(server: &MockServer) -> TeslaAdapter {
        let mut adapter =
            TeslaAdapter::new(TeslaRegion::NorthAmerica).with_base_url(&server.base_url());
        adapter.register_vehicle("tesla-001", "vid-123", "5YJ3E1EA1NF000001", test_token());
        adapter
    }

    // 1. wake_up sends correct POST with Bearer token
    #[tokio::test]
    async fn wake_up_sends_correct_post_with_bearer() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/wake_up")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        let entry = adapter.get_entry("tesla-001").unwrap();
        adapter.wake_up(entry).await.unwrap();
        mock.assert();
    }

    // 2. wake_up success parses response
    #[tokio::test]
    async fn wake_up_success_parses_response() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/1/vehicles/vid-123/wake_up");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        let entry = adapter.get_entry("tesla-001").unwrap();
        assert!(adapter.wake_up(entry).await.is_ok());
    }

    // 3. vehicle_data sends correct GET with Bearer token
    #[tokio::test]
    async fn vehicle_data_sends_correct_get_with_bearer() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/1/vehicles/vid-123/vehicle_data")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(vehicle_data_body());
        });
        let adapter = setup(&server);
        let entry = adapter.get_entry("tesla-001").unwrap();
        adapter.vehicle_data(entry).await.unwrap();
        mock.assert();
    }

    // 4. vehicle_data parses drive_state (location)
    #[tokio::test]
    async fn vehicle_data_parses_location() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/api/1/vehicles/vid-123/vehicle_data");
            then.status(200).json_body(vehicle_data_body());
        });
        let adapter = setup(&server);
        let entry = adapter.get_entry("tesla-001").unwrap();
        let state = adapter.vehicle_data(entry).await.unwrap();
        let (lat, lng) = state.location.unwrap();
        assert!((lat - 37.38).abs() < 0.01);
        assert!((lng - (-122.08)).abs() < 0.01);
    }

    // 5. vehicle_data parses charge_state (battery, range, charging)
    #[tokio::test]
    async fn vehicle_data_parses_charge_state() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/api/1/vehicles/vid-123/vehicle_data");
            then.status(200).json_body(vehicle_data_body());
        });
        let adapter = setup(&server);
        let entry = adapter.get_entry("tesla-001").unwrap();
        let state = adapter.vehicle_data(entry).await.unwrap();
        assert_eq!(state.charge_level_pct, Some(78.0));
        assert!(!state.charging); // "Disconnected"
                                  // 192.5 miles * 1.60934 ≈ 309.8
        let range = state.range_km.unwrap();
        assert!((range - 309.8).abs() < 1.0);
    }

    // 6. vehicle_data parses vehicle_state (locked, odometer)
    #[tokio::test]
    async fn vehicle_data_parses_vehicle_state() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/api/1/vehicles/vid-123/vehicle_data");
            then.status(200).json_body(vehicle_data_body());
        });
        let adapter = setup(&server);
        let entry = adapter.get_entry("tesla-001").unwrap();
        let state = adapter.vehicle_data(entry).await.unwrap();
        assert!(state.locked);
        // 15534.2 miles * 1.60934 ≈ 24999.8
        let odo = state.odometer_km.unwrap();
        assert!((odo - 24999.8).abs() < 5.0);
    }

    // 7. vehicle_data parses climate_state
    #[tokio::test]
    async fn vehicle_data_parses_climate_state() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/api/1/vehicles/vid-123/vehicle_data");
            then.status(200).json_body(vehicle_data_body());
        });
        let adapter = setup(&server);
        let entry = adapter.get_entry("tesla-001").unwrap();
        let state = adapter.vehicle_data(entry).await.unwrap();
        assert!(!state.climate_on);
    }

    // 8. door_lock sends correct command
    #[tokio::test]
    async fn door_lock_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/door_lock")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "lock", json!(true))
            .await
            .unwrap();
        mock.assert();
    }

    // 9. door_unlock sends correct command
    #[tokio::test]
    async fn door_unlock_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/door_unlock")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "lock", json!(false))
            .await
            .unwrap();
        mock.assert();
    }

    // 10. climate on/off sends correct commands
    #[tokio::test]
    async fn climate_on_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/auto_conditioning_start");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "climate", json!(true))
            .await
            .unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn climate_off_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/auto_conditioning_stop");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "climate", json!(false))
            .await
            .unwrap();
        mock.assert();
    }

    // 11. set_charge_limit sends correct body
    #[tokio::test]
    async fn set_charge_limit_sends_correct_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/set_charge_limit")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"percent": 90}));
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "charge_limit", json!(90))
            .await
            .unwrap();
        mock.assert();
    }

    // 12. honk_horn sends correct command
    #[tokio::test]
    async fn honk_horn_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/honk_horn")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "honk", Value::Null)
            .await
            .unwrap();
        mock.assert();
    }

    // 13. flash_lights sends correct command
    #[tokio::test]
    async fn flash_lights_sends_correct_command() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/flash_lights")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "flash", Value::Null)
            .await
            .unwrap();
        mock.assert();
    }

    // 14. actuate_trunk sends correct body
    #[tokio::test]
    async fn actuate_trunk_sends_correct_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/actuate_trunk")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"which_trunk": "rear"}));
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "trunk", Value::Null)
            .await
            .unwrap();
        mock.assert();
    }

    // 15. Command failure (result: false) returns error with reason
    #[tokio::test]
    async fn command_failure_returns_error_with_reason() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/door_lock");
            then.status(200)
                .json_body(failure_body("vehicle unavailable"));
        });
        let adapter = setup(&server);
        let err = adapter.set_property("tesla-001", "lock", json!(true)).await;
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("vehicle unavailable"));
    }

    // 16. 429 rate limit returns error
    #[tokio::test]
    async fn rate_limit_429_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/honk_horn");
            then.status(429).body("rate limited");
        });
        let adapter = setup(&server);
        let err = adapter.set_property("tesla-001", "honk", Value::Null).await;
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("429"));
    }

    // 17. 500 server error returns error
    #[tokio::test]
    async fn server_error_500_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/1/vehicles/vid-123/wake_up");
            then.status(500).body("internal server error");
        });
        let adapter = setup(&server);
        let err = adapter.turn_on("tesla-001").await;
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("500"));
    }

    // 18. get_state (DeviceController) calls vehicle_data
    #[tokio::test]
    async fn get_state_calls_vehicle_data() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/1/vehicles/vid-123/vehicle_data");
            then.status(200).json_body(vehicle_data_body());
        });
        let adapter = setup(&server);
        let state = adapter.get_state("tesla-001").await.unwrap();
        assert_eq!(state["locked"], true);
        assert_eq!(state["charge_level_pct"], 78.0);
        mock.assert();
    }

    // 19. turn_on (DeviceController) calls wake_up
    #[tokio::test]
    async fn turn_on_calls_wake_up() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/1/vehicles/vid-123/wake_up");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter.turn_on("tesla-001").await.unwrap();
        mock.assert();
    }

    // 20. set_property routes to correct command
    #[tokio::test]
    async fn set_property_routes_lock_to_door_lock() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/1/vehicles/vid-123/command/door_lock");
            then.status(200).json_body(success_body());
        });
        let adapter = setup(&server);
        adapter
            .set_property("tesla-001", "lock", json!(true))
            .await
            .unwrap();
        mock.assert();
    }

    // 21. Unknown property returns error
    #[tokio::test]
    async fn unknown_property_returns_error() {
        let server = MockServer::start();
        let adapter = setup(&server);
        let err = adapter
            .set_property("tesla-001", "radio_volume", json!(50))
            .await;
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("unsupported Tesla property"));
    }

    // 22. Unknown vehicle returns error
    #[tokio::test]
    async fn unknown_vehicle_returns_error() {
        let server = MockServer::start();
        let adapter = setup(&server);
        assert!(adapter.get_state("nonexistent").await.is_err());
        assert!(adapter.turn_on("nonexistent").await.is_err());
        assert!(adapter
            .set_property("nonexistent", "lock", json!(true))
            .await
            .is_err());
    }

    // 23. Region base URLs are correct
    #[test]
    fn region_base_urls_are_correct() {
        assert_eq!(
            TeslaRegion::NorthAmerica.base_url(),
            "https://fleet-api.prd.na.vn.cloud.tesla.com"
        );
        assert_eq!(
            TeslaRegion::Europe.base_url(),
            "https://fleet-api.prd.eu.vn.cloud.tesla.com"
        );
        assert_eq!(
            TeslaRegion::China.base_url(),
            "https://fleet-api.prd.cn.vn.cloud.tesla.com"
        );
    }

    // 24. supported_type returns "Vehicle"
    #[test]
    fn supported_type_is_vehicle() {
        let adapter = TeslaAdapter::new(TeslaRegion::NorthAmerica);
        assert_eq!(adapter.supported_type(), "Vehicle");
    }

    // 25. turn_off always errors
    #[tokio::test]
    async fn turn_off_always_errors() {
        let server = MockServer::start();
        let adapter = setup(&server);
        let err = adapter.turn_off("tesla-001").await;
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("does not support remote shutdown"));
    }

    // 26. vehicle_data with charging state "Charging"
    #[tokio::test]
    async fn vehicle_data_charging_state_active() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/api/1/vehicles/vid-123/vehicle_data");
            then.status(200).json_body(json!({
                "response": {
                    "drive_state": {"latitude": 40.0, "longitude": -74.0},
                    "charge_state": {
                        "battery_level": 45,
                        "battery_range": 100.0,
                        "charging_state": "Charging"
                    },
                    "vehicle_state": {"locked": false, "odometer": 5000.0},
                    "climate_state": {"is_climate_on": true}
                }
            }));
        });
        let adapter = setup(&server);
        let entry = adapter.get_entry("tesla-001").unwrap();
        let state = adapter.vehicle_data(entry).await.unwrap();
        assert!(state.charging);
        assert!(state.climate_on);
        assert!(!state.locked);
        assert_eq!(state.charge_level_pct, Some(45.0));
    }

    // 27. with_base_url trims trailing slash
    #[test]
    fn with_base_url_trims_trailing_slash() {
        let adapter =
            TeslaAdapter::new(TeslaRegion::NorthAmerica).with_base_url("http://localhost:1234/");
        assert_eq!(adapter.base_url, "http://localhost:1234");
    }

    // 28. endpoint builds correct URL
    #[test]
    fn endpoint_builds_correct_url() {
        let adapter =
            TeslaAdapter::new(TeslaRegion::NorthAmerica).with_base_url("http://localhost:5555");
        let url = adapter.endpoint("api/1/vehicles/v1/wake_up");
        assert_eq!(url, "http://localhost:5555/api/1/vehicles/v1/wake_up");
    }
}
