//! SmartCar unified API adapter
//!
//! Controls 40+ vehicle brands via the SmartCar REST API (v2.0).
//! Capabilities: lock/unlock, charge control, read vehicle state.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::iot::control::DeviceController;
use crate::iot::vehicle::oauth::TokenStore;
use crate::iot::vehicle::types::VehicleState;

/// Brands supported by SmartCar's unified API.
pub const SUPPORTED_MAKES: &[&str] = &[
    "ACURA",
    "AUDI",
    "BMW",
    "BUICK",
    "CADILLAC",
    "CHEVROLET",
    "CHRYSLER",
    "DODGE",
    "FIAT",
    "FORD",
    "GMC",
    "GENESIS",
    "HONDA",
    "HYUNDAI",
    "INFINITI",
    "JAGUAR",
    "JEEP",
    "KIA",
    "LAND_ROVER",
    "LEXUS",
    "LINCOLN",
    "MAZDA",
    "MERCEDES",
    "MINI",
    "MITSUBISHI",
    "NISSAN",
    "POLESTAR",
    "PORSCHE",
    "RAM",
    "RIVIAN",
    "SUBARU",
    "TESLA",
    "TOYOTA",
    "VOLKSWAGEN",
    "VOLVO",
];

const DEFAULT_BASE_URL: &str = "https://api.smartcar.com/v2.0";

// ---------------------------------------------------------------------------
// API response structs (deserialize-only)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LocationResponse {
    latitude: f64,
    longitude: f64,
}

#[derive(Debug, Deserialize)]
struct OdometerResponse {
    distance: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FuelResponse {
    percent_remaining: f64,
    range: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatteryResponse {
    percent_remaining: f64,
    range: f64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ActionResponse {
    status: String,
}

#[derive(Debug, Deserialize)]
struct SmartCarError {
    error: String,
    message: String,
}

// ---------------------------------------------------------------------------
// Internal vehicle entry
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct SmartCarVehicleEntry {
    vehicle_id: String,
    make: String,
    model: String,
    year: u16,
    is_electric: bool,
    token: TokenStore,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct SmartCarAdapter {
    vehicles: HashMap<String, SmartCarVehicleEntry>,
    client: reqwest::Client,
    base_url: String,
}

impl SmartCarAdapter {
    pub fn new() -> Self {
        Self {
            vehicles: HashMap::new(),
            client: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Override the base URL (for httpmock testing).
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    /// Register a vehicle with its OAuth token.
    #[allow(clippy::too_many_arguments)]
    pub fn register_vehicle(
        &mut self,
        device_id: &str,
        vehicle_id: &str,
        make: &str,
        model: &str,
        year: u16,
        is_electric: bool,
        token: TokenStore,
    ) {
        self.vehicles.insert(
            device_id.to_string(),
            SmartCarVehicleEntry {
                vehicle_id: vehicle_id.to_string(),
                make: make.to_string(),
                model: model.to_string(),
                year,
                is_electric,
                token,
            },
        );
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn get_entry(&self, device_id: &str) -> anyhow::Result<&SmartCarVehicleEntry> {
        self.vehicles
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown vehicle: {}", device_id))
    }

    /// Parse an error body from the SmartCar API.
    async fn check_response(&self, resp: reqwest::Response) -> anyhow::Result<reqwest::Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        if status.as_u16() == 401 {
            anyhow::bail!("authentication failed: invalid or expired token");
        }
        if status.as_u16() == 429 {
            anyhow::bail!("rate limit exceeded");
        }
        let body = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<SmartCarError>(&body) {
            anyhow::bail!("SmartCar error ({}): {}", err.error, err.message);
        }
        anyhow::bail!("SmartCar API error ({}): {}", status, body);
    }

    async fn get_location(&self, entry: &SmartCarVehicleEntry) -> anyhow::Result<(f64, f64)> {
        let url = self.endpoint(&format!("/vehicles/{}/location", entry.vehicle_id));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&entry.token.access_token)
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        let loc: LocationResponse = resp.json().await?;
        Ok((loc.latitude, loc.longitude))
    }

    async fn get_odometer(&self, entry: &SmartCarVehicleEntry) -> anyhow::Result<f64> {
        let url = self.endpoint(&format!("/vehicles/{}/odometer", entry.vehicle_id));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&entry.token.access_token)
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        let odo: OdometerResponse = resp.json().await?;
        Ok(odo.distance)
    }

    async fn get_fuel(&self, entry: &SmartCarVehicleEntry) -> anyhow::Result<(f64, f64)> {
        let url = self.endpoint(&format!("/vehicles/{}/fuel", entry.vehicle_id));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&entry.token.access_token)
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        let fuel: FuelResponse = resp.json().await?;
        Ok((fuel.percent_remaining, fuel.range))
    }

    async fn get_battery(&self, entry: &SmartCarVehicleEntry) -> anyhow::Result<(f64, f64)> {
        let url = self.endpoint(&format!("/vehicles/{}/battery", entry.vehicle_id));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&entry.token.access_token)
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        let bat: BatteryResponse = resp.json().await?;
        Ok((bat.percent_remaining, bat.range))
    }

    async fn security_action(
        &self,
        entry: &SmartCarVehicleEntry,
        action: &str,
    ) -> anyhow::Result<()> {
        let url = self.endpoint(&format!("/vehicles/{}/security", entry.vehicle_id));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&entry.token.access_token)
            .json(&serde_json::json!({ "action": action }))
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        let _action_resp: ActionResponse = resp.json().await?;
        Ok(())
    }

    async fn charge_action(
        &self,
        entry: &SmartCarVehicleEntry,
        action: &str,
    ) -> anyhow::Result<()> {
        let url = self.endpoint(&format!("/vehicles/{}/charge", entry.vehicle_id));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&entry.token.access_token)
            .json(&serde_json::json!({ "action": action }))
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        let _action_resp: ActionResponse = resp.json().await?;
        Ok(())
    }

    async fn set_charge_limit_pct(
        &self,
        entry: &SmartCarVehicleEntry,
        limit: f64,
    ) -> anyhow::Result<()> {
        let url = self.endpoint(&format!("/vehicles/{}/charge/limit", entry.vehicle_id));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&entry.token.access_token)
            .json(&serde_json::json!({ "limit": limit }))
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        let _action_resp: ActionResponse = resp.json().await?;
        Ok(())
    }

    async fn fetch_full_state(&self, entry: &SmartCarVehicleEntry) -> anyhow::Result<VehicleState> {
        let location = self.get_location(entry).await?;
        let odometer = self.get_odometer(entry).await?;

        let (pct, range) = if entry.is_electric {
            self.get_battery(entry).await?
        } else {
            self.get_fuel(entry).await?
        };

        Ok(VehicleState {
            locked: false,     // SmartCar does not expose lock state in GET
            climate_on: false, // SmartCar does not expose climate state in GET
            charge_level_pct: Some(pct * 100.0),
            location: Some(location),
            odometer_km: Some(odometer),
            charging: false, // SmartCar does not expose charging state in GET
            range_km: Some(range),
        })
    }
}

// ---------------------------------------------------------------------------
// DeviceController implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for SmartCarAdapter {
    async fn turn_on(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("SmartCar API does not support remote engine start")
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("SmartCar API does not support remote engine stop")
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
                let action = if lock { "LOCK" } else { "UNLOCK" };
                self.security_action(entry, action).await
            }

            "charge" => {
                let start = value.as_bool().unwrap_or(true);
                let action = if start { "START" } else { "STOP" };
                self.charge_action(entry, action).await
            }
            "charge_limit" => {
                let limit = value
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("charge_limit must be a number"))?;
                self.set_charge_limit_pct(entry, limit).await
            }
            _ => anyhow::bail!("unsupported SmartCar property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let entry = self.get_entry(device_id)?;
        let state = self.fetch_full_state(entry).await?;
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
    use serde_json::json;

    fn make_token() -> TokenStore {
        TokenStore {
            access_token: "test-token".to_string(),
            refresh_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scope: None,
        }
    }

    fn setup_adapter(base_url: &str) -> SmartCarAdapter {
        let mut adapter = SmartCarAdapter::new().with_base_url(base_url);
        adapter.register_vehicle(
            "sc-001",
            "vid-abc",
            "BMW",
            "330i",
            2023,
            false,
            make_token(),
        );
        adapter
    }

    fn setup_electric_adapter(base_url: &str) -> SmartCarAdapter {
        let mut adapter = SmartCarAdapter::new().with_base_url(base_url);
        adapter.register_vehicle(
            "sc-ev",
            "vid-ev",
            "TESLA",
            "Model 3",
            2024,
            true,
            make_token(),
        );
        adapter
    }

    // 1. get_location sends GET with Bearer token
    #[tokio::test]
    async fn get_location_sends_get_with_bearer() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/vehicles/vid-abc/location")
                .header("authorization", "Bearer test-token");
            then.status(200)
                .json_body(json!({"latitude": 37.77, "longitude": -122.42}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        adapter.get_location(entry).await.unwrap();
        mock.assert();
    }

    // 2. get_location parses latitude/longitude
    #[tokio::test]
    async fn get_location_parses_coords() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/location");
            then.status(200)
                .json_body(json!({"latitude": 48.85, "longitude": 2.35}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        let (lat, lng) = adapter.get_location(entry).await.unwrap();
        assert!((lat - 48.85).abs() < 0.001);
        assert!((lng - 2.35).abs() < 0.001);
    }

    // 3. get_odometer sends GET and parses distance
    #[tokio::test]
    async fn get_odometer_parses_distance() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/vehicles/vid-abc/odometer")
                .header("authorization", "Bearer test-token");
            then.status(200).json_body(json!({"distance": 42000.5}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        let dist = adapter.get_odometer(entry).await.unwrap();
        assert!((dist - 42000.5).abs() < 0.1);
        mock.assert();
    }

    // 4. get_fuel sends GET and parses percentRemaining + range
    #[tokio::test]
    async fn get_fuel_parses_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/vehicles/vid-abc/fuel")
                .header("authorization", "Bearer test-token");
            then.status(200)
                .json_body(json!({"percentRemaining": 0.65, "range": 320.5}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        let (pct, range) = adapter.get_fuel(entry).await.unwrap();
        assert!((pct - 0.65).abs() < 0.001);
        assert!((range - 320.5).abs() < 0.1);
        mock.assert();
    }

    // 5. get_battery sends GET and parses percentRemaining + range
    #[tokio::test]
    async fn get_battery_parses_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/vehicles/vid-ev/battery")
                .header("authorization", "Bearer test-token");
            then.status(200)
                .json_body(json!({"percentRemaining": 0.80, "range": 250.0}));
        });
        let adapter = setup_electric_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-ev").unwrap();
        let (pct, range) = adapter.get_battery(entry).await.unwrap();
        assert!((pct - 0.80).abs() < 0.001);
        assert!((range - 250.0).abs() < 0.1);
        mock.assert();
    }

    // 6. lock sends POST /vehicles/{id}/security with {"action":"LOCK"}
    #[tokio::test]
    async fn lock_sends_security_lock() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/vehicles/vid-abc/security")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"action": "LOCK"}));
            then.status(200).json_body(json!({"status": "success"}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        adapter.security_action(entry, "LOCK").await.unwrap();
        mock.assert();
    }

    // 7. unlock sends POST with {"action":"UNLOCK"}
    #[tokio::test]
    async fn unlock_sends_security_unlock() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/vehicles/vid-abc/security")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"action": "UNLOCK"}));
            then.status(200).json_body(json!({"status": "success"}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        adapter.security_action(entry, "UNLOCK").await.unwrap();
        mock.assert();
    }

    // 8. start_charge sends POST /vehicles/{id}/charge with {"action":"START"}
    #[tokio::test]
    async fn start_charge_sends_charge_start() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/vehicles/vid-abc/charge")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"action": "START"}));
            then.status(200).json_body(json!({"status": "success"}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        adapter.charge_action(entry, "START").await.unwrap();
        mock.assert();
    }

    // 9. stop_charge sends POST with {"action":"STOP"}
    #[tokio::test]
    async fn stop_charge_sends_charge_stop() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/vehicles/vid-abc/charge")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"action": "STOP"}));
            then.status(200).json_body(json!({"status": "success"}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        adapter.charge_action(entry, "STOP").await.unwrap();
        mock.assert();
    }

    // 10. set_charge_limit sends POST /vehicles/{id}/charge/limit with {"limit":0.8}
    #[tokio::test]
    async fn set_charge_limit_sends_correct_body() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/vehicles/vid-abc/charge/limit")
                .header("authorization", "Bearer test-token")
                .json_body(json!({"limit": 0.8}));
            then.status(200).json_body(json!({"status": "success"}));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        adapter.set_charge_limit_pct(entry, 0.8).await.unwrap();
        mock.assert();
    }

    // 11. fetch_full_state aggregates location+odometer+fuel for gas car
    #[tokio::test]
    async fn fetch_full_state_gas_car() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/location");
            then.status(200)
                .json_body(json!({"latitude": 37.77, "longitude": -122.42}));
        });
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/odometer");
            then.status(200).json_body(json!({"distance": 42000.5}));
        });
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/fuel");
            then.status(200)
                .json_body(json!({"percentRemaining": 0.65, "range": 320.5}));
        });

        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        let state = adapter.fetch_full_state(entry).await.unwrap();
        assert!((state.charge_level_pct.unwrap() - 65.0).abs() < 0.1);
        assert!((state.range_km.unwrap() - 320.5).abs() < 0.1);
        assert!((state.odometer_km.unwrap() - 42000.5).abs() < 0.1);
        let (lat, lng) = state.location.unwrap();
        assert!((lat - 37.77).abs() < 0.01);
        assert!((lng - (-122.42)).abs() < 0.01);
    }

    // 12. fetch_full_state aggregates location+odometer+battery for electric car
    #[tokio::test]
    async fn fetch_full_state_electric_car() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-ev/location");
            then.status(200)
                .json_body(json!({"latitude": 40.71, "longitude": -74.00}));
        });
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-ev/odometer");
            then.status(200).json_body(json!({"distance": 15000.0}));
        });
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-ev/battery");
            then.status(200)
                .json_body(json!({"percentRemaining": 0.80, "range": 250.0}));
        });
        let adapter = setup_electric_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-ev").unwrap();
        let state = adapter.fetch_full_state(entry).await.unwrap();
        assert!((state.charge_level_pct.unwrap() - 80.0).abs() < 0.1);
        assert!((state.range_km.unwrap() - 250.0).abs() < 0.1);
        assert!((state.odometer_km.unwrap() - 15000.0).abs() < 0.1);
    }

    // 13. API error response parsed correctly
    #[tokio::test]
    async fn api_error_response_parsed() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/location");
            then.status(409).json_body(json!({
                "error": "VEHICLE_STATE",
                "message": "Vehicle is not reachable"
            }));
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        let err = adapter.get_location(entry).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("VEHICLE_STATE"));
        assert!(msg.contains("Vehicle is not reachable"));
    }

    // 14. 401 returns auth error
    #[tokio::test]
    async fn auth_error_on_401() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/location");
            then.status(401).body("Unauthorized");
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        let err = adapter.get_location(entry).await.unwrap_err();
        assert!(err.to_string().contains("authentication failed"));
    }

    // 15. 429 returns rate limit error
    #[tokio::test]
    async fn rate_limit_on_429() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/odometer");
            then.status(429).body("Too Many Requests");
        });
        let adapter = setup_adapter(&server.base_url());
        let entry = adapter.get_entry("sc-001").unwrap();
        let err = adapter.get_odometer(entry).await.unwrap_err();
        assert!(err.to_string().contains("rate limit exceeded"));
    }

    // 16. get_state (DeviceController) returns VehicleState JSON
    #[tokio::test]
    async fn device_controller_get_state() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/location");
            then.status(200)
                .json_body(json!({"latitude": 37.77, "longitude": -122.42}));
        });
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/odometer");
            then.status(200).json_body(json!({"distance": 10000.0}));
        });
        server.mock(|when, then| {
            when.method(GET).path("/vehicles/vid-abc/fuel");
            then.status(200)
                .json_body(json!({"percentRemaining": 0.50, "range": 200.0}));
        });
        let adapter = setup_adapter(&server.base_url());
        let state = adapter.get_state("sc-001").await.unwrap();
        assert!((state["charge_level_pct"].as_f64().unwrap() - 50.0).abs() < 0.1);
        assert!(state["location"].is_array());
        assert!(state["odometer_km"].as_f64().is_some());
        assert!((state["range_km"].as_f64().unwrap() - 200.0).abs() < 0.1);
    }

    // 17. set_property "lock" true calls security LOCK
    #[tokio::test]
    async fn set_property_lock_true_calls_lock() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/vehicles/vid-abc/security")
                .json_body(json!({"action": "LOCK"}));
            then.status(200).json_body(json!({"status": "success"}));
        });
        let adapter = setup_adapter(&server.base_url());
        adapter
            .set_property("sc-001", "lock", json!(true))
            .await
            .unwrap();
        mock.assert();
    }

    // 18. set_property "lock" false calls security UNLOCK
    #[tokio::test]
    async fn set_property_lock_false_calls_unlock() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/vehicles/vid-abc/security")
                .json_body(json!({"action": "UNLOCK"}));
            then.status(200).json_body(json!({"status": "success"}));
        });
        let adapter = setup_adapter(&server.base_url());
        adapter
            .set_property("sc-001", "lock", json!(false))
            .await
            .unwrap();
        mock.assert();
    }

    // 19. set_property "charge_limit" calls set_charge_limit_pct
    #[tokio::test]
    async fn set_property_charge_limit() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/vehicles/vid-abc/charge/limit")
                .json_body(json!({"limit": 0.9}));
            then.status(200).json_body(json!({"status": "success"}));
        });
        let adapter = setup_adapter(&server.base_url());
        adapter
            .set_property("sc-001", "charge_limit", json!(0.9))
            .await
            .unwrap();
        mock.assert();
    }

    // 20. set_property unsupported returns error
    #[tokio::test]
    async fn set_property_unsupported_errors() {
        let server = MockServer::start();
        let adapter = setup_adapter(&server.base_url());
        let err = adapter
            .set_property("sc-001", "sunroof", json!(true))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unsupported SmartCar property"));
    }

    // 21. Unknown vehicle returns error
    #[tokio::test]
    async fn unknown_vehicle_errors() {
        let adapter = SmartCarAdapter::new();
        let err = adapter.get_state("nonexistent").await.unwrap_err();
        assert!(err.to_string().contains("unknown vehicle"));
        let err2 = adapter
            .set_property("nonexistent", "lock", json!(true))
            .await
            .unwrap_err();
        assert!(err2.to_string().contains("unknown vehicle"));
    }

    // 22. SUPPORTED_MAKES contains expected brands
    #[test]
    fn supported_makes_contains_expected() {
        assert!(SUPPORTED_MAKES.contains(&"BMW"));
        assert!(SUPPORTED_MAKES.contains(&"TESLA"));
        assert!(SUPPORTED_MAKES.contains(&"TOYOTA"));
        assert!(SUPPORTED_MAKES.contains(&"FORD"));
        assert!(SUPPORTED_MAKES.contains(&"RIVIAN"));
        assert!(SUPPORTED_MAKES.contains(&"PORSCHE"));
        assert!(SUPPORTED_MAKES.contains(&"MERCEDES"));
        assert!(!SUPPORTED_MAKES.contains(&"UNKNOWN_BRAND"));
    }

    // Extra: supported_type returns "Vehicle"
    #[test]
    fn supported_type_is_vehicle() {
        assert_eq!(SmartCarAdapter::new().supported_type(), "Vehicle");
    }

    // Extra: turn_on / turn_off rejected
    #[tokio::test]
    async fn turn_on_rejected() {
        let adapter = SmartCarAdapter::new();
        assert!(adapter.turn_on("any").await.is_err());
    }

    #[tokio::test]
    async fn turn_off_rejected() {
        let adapter = SmartCarAdapter::new();
        assert!(adapter.turn_off("any").await.is_err());
    }
}
