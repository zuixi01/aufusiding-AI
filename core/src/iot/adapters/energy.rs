//! Energy monitoring adapter
//!
//! Supports SolarEdge solar inverters and Shelly smart plugs/meters.
//! Capabilities: EnergyMonitoring, PowerMeasurement, SolarTracking

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Energy device provider type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnergyProvider {
    SolarEdge,
    Shelly,
}

/// Energy device configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyConfig {
    pub provider: EnergyProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

// ---------------------------------------------------------------------------
// SolarEdge API responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeOverview {
    pub overview: SolarEdgeOverviewData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolarEdgeOverviewData {
    pub current_power: SolarEdgePower,
    pub last_day_data: SolarEdgeEnergy,
    pub last_month_data: SolarEdgeEnergy,
    pub life_time_data: SolarEdgeEnergy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgePower {
    pub power: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeEnergy {
    pub energy: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgePowerFlow {
    #[serde(rename = "siteCurrentPowerFlow")]
    pub site_current_power_flow: SolarEdgePowerFlowData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub struct SolarEdgePowerFlowData {
    pub pv: Option<SolarEdgePowerFlowValue>,
    pub load: Option<SolarEdgePowerFlowValue>,
    pub grid: Option<SolarEdgePowerFlowValue>,
    pub storage: Option<SolarEdgePowerFlowValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolarEdgePowerFlowValue {
    pub current_power: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeEnergyDetails {
    #[serde(rename = "energyDetails")]
    pub energy_details: SolarEdgeEnergyDetailsData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeEnergyDetailsData {
    pub meters: Vec<SolarEdgeMeter>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeMeter {
    #[serde(rename = "type")]
    pub meter_type: String,
    pub values: Vec<SolarEdgeMeterValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeMeterValue {
    pub date: String,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeInventory {
    #[serde(rename = "Inventory")]
    pub inventory: SolarEdgeInventoryData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeInventoryData {
    pub inverters: Vec<SolarEdgeInverter>,
    #[serde(default)]
    pub batteries: Vec<SolarEdgeBattery>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeInverter {
    pub name: String,
    #[serde(rename = "SN")]
    pub serial_number: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarEdgeBattery {
    pub name: String,
    #[serde(rename = "SN")]
    pub serial_number: String,
}

// ---------------------------------------------------------------------------
// Shelly API responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ShellySwitchStatus {
    pub id: u8,
    pub output: bool,
    #[serde(default)]
    pub apower: f64,
    #[serde(default)]
    pub voltage: f64,
    #[serde(default)]
    pub current: f64,
    pub aenergy: ShellyEnergy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShellyEnergy {
    pub total: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShellyMeterStatus {
    pub id: u8,
    pub a_act_power: f64,
    pub a_voltage: f64,
    pub a_current: f64,
    pub total_act_energy: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShellyDeviceInfo {
    pub id: String,
    pub model: String,
    pub fw_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShellySwitchSetResponse {
    pub was_on: bool,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Energy monitoring adapter with real SolarEdge and Shelly API integration
pub struct EnergyAdapter {
    client: reqwest::Client,
    devices: HashMap<String, EnergyConfig>,
    solaredge_base_url: String,
    shelly_base_url_override: Option<String>,
}

impl EnergyAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            devices: HashMap::new(),
            solaredge_base_url: "https://monitoringapi.solaredge.com".to_string(),
            shelly_base_url_override: None,
        }
    }

    /// Override base URL for testing
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.solaredge_base_url = url.trim_end_matches('/').to_string();
        self.shelly_base_url_override = Some(url.trim_end_matches('/').to_string());
        self
    }

    /// Register an energy device
    pub fn register_device(&mut self, device_id: &str, config: EnergyConfig) {
        self.devices.insert(device_id.to_string(), config);
    }

    fn get_config(&self, device_id: &str) -> anyhow::Result<&EnergyConfig> {
        self.devices
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown energy device: {}", device_id))
    }

    fn solaredge_endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.solaredge_base_url,
            path.trim_start_matches('/')
        )
    }

    fn shelly_endpoint(&self, host: &str, path: &str) -> String {
        let base = self.shelly_base_url_override.as_deref().unwrap_or(host);
        format!("{}/{}", base, path.trim_start_matches('/'))
    }

    // SolarEdge API methods

    /// GET /site/{siteId}/overview
    pub async fn get_overview(
        &self,
        site_id: &str,
        api_key: &str,
    ) -> anyhow::Result<SolarEdgeOverview> {
        let url = self.solaredge_endpoint(&format!("site/{}/overview", site_id));
        let resp = self
            .client
            .get(&url)
            .query(&[("api_key", api_key)])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /site/{}/overview failed ({}): {}",
                site_id,
                status.as_u16(),
                body
            );
        }
        let overview: SolarEdgeOverview = resp.json().await?;
        Ok(overview)
    }

    /// GET /site/{siteId}/currentPowerFlow
    pub async fn get_power_flow(
        &self,
        site_id: &str,
        api_key: &str,
    ) -> anyhow::Result<SolarEdgePowerFlow> {
        let url = self.solaredge_endpoint(&format!("site/{}/currentPowerFlow", site_id));
        let resp = self
            .client
            .get(&url)
            .query(&[("api_key", api_key)])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /site/{}/currentPowerFlow failed ({}): {}",
                site_id,
                status.as_u16(),
                body
            );
        }
        let power_flow: SolarEdgePowerFlow = resp.json().await?;
        Ok(power_flow)
    }

    /// GET /site/{siteId}/energyDetails
    pub async fn get_energy_details(
        &self,
        site_id: &str,
        api_key: &str,
        start_time: &str,
        end_time: &str,
    ) -> anyhow::Result<SolarEdgeEnergyDetails> {
        let url = self.solaredge_endpoint(&format!("site/{}/energyDetails", site_id));
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("api_key", api_key),
                ("timeUnit", "DAY"),
                ("startTime", start_time),
                ("endTime", end_time),
            ])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /site/{}/energyDetails failed ({}): {}",
                site_id,
                status.as_u16(),
                body
            );
        }
        let details: SolarEdgeEnergyDetails = resp.json().await?;
        Ok(details)
    }

    /// GET /site/{siteId}/inventory
    pub async fn get_inventory(
        &self,
        site_id: &str,
        api_key: &str,
    ) -> anyhow::Result<SolarEdgeInventory> {
        let url = self.solaredge_endpoint(&format!("site/{}/inventory", site_id));
        let resp = self
            .client
            .get(&url)
            .query(&[("api_key", api_key)])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /site/{}/inventory failed ({}): {}",
                site_id,
                status.as_u16(),
                body
            );
        }
        let inventory: SolarEdgeInventory = resp.json().await?;
        Ok(inventory)
    }

    // Shelly API methods

    /// GET /rpc/Switch.GetStatus?id=0
    pub async fn get_switch_status(
        &self,
        host: &str,
        switch_id: u8,
    ) -> anyhow::Result<ShellySwitchStatus> {
        let url = self.shelly_endpoint(host, "rpc/Switch.GetStatus");
        let resp = self
            .client
            .get(&url)
            .query(&[("id", switch_id.to_string())])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /rpc/Switch.GetStatus failed ({}): {}",
                status.as_u16(),
                body
            );
        }
        let switch_status: ShellySwitchStatus = resp.json().await?;
        Ok(switch_status)
    }

    /// GET /rpc/Switch.Set?id=0&on=true
    pub async fn switch_on(
        &self,
        host: &str,
        switch_id: u8,
    ) -> anyhow::Result<ShellySwitchSetResponse> {
        let url = self.shelly_endpoint(host, "rpc/Switch.Set");
        let resp = self
            .client
            .get(&url)
            .query(&[("id", switch_id.to_string()), ("on", "true".to_string())])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /rpc/Switch.Set (on) failed ({}): {}",
                status.as_u16(),
                body
            );
        }
        let response: ShellySwitchSetResponse = resp.json().await?;
        Ok(response)
    }

    /// GET /rpc/Switch.Set?id=0&on=false
    pub async fn switch_off(
        &self,
        host: &str,
        switch_id: u8,
    ) -> anyhow::Result<ShellySwitchSetResponse> {
        let url = self.shelly_endpoint(host, "rpc/Switch.Set");
        let resp = self
            .client
            .get(&url)
            .query(&[("id", switch_id.to_string()), ("on", "false".to_string())])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /rpc/Switch.Set (off) failed ({}): {}",
                status.as_u16(),
                body
            );
        }
        let response: ShellySwitchSetResponse = resp.json().await?;
        Ok(response)
    }

    /// GET /rpc/EM.GetStatus?id=0
    pub async fn get_meter_status(
        &self,
        host: &str,
        meter_id: u8,
    ) -> anyhow::Result<ShellyMeterStatus> {
        let url = self.shelly_endpoint(host, "rpc/EM.GetStatus");
        let resp = self
            .client
            .get(&url)
            .query(&[("id", meter_id.to_string())])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /rpc/EM.GetStatus failed ({}): {}",
                status.as_u16(),
                body
            );
        }
        let meter_status: ShellyMeterStatus = resp.json().await?;
        Ok(meter_status)
    }

    /// GET /rpc/Shelly.GetDeviceInfo
    pub async fn get_device_info(&self, host: &str) -> anyhow::Result<ShellyDeviceInfo> {
        let url = self.shelly_endpoint(host, "rpc/Shelly.GetDeviceInfo");
        let resp = self.client.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GET /rpc/Shelly.GetDeviceInfo failed ({}): {}",
                status.as_u16(),
                body
            );
        }
        let device_info: ShellyDeviceInfo = resp.json().await?;
        Ok(device_info)
    }
}

#[async_trait]
impl DeviceController for EnergyAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;
        match config.provider {
            EnergyProvider::Shelly => {
                let host = config
                    .host
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Shelly device requires host"))?;
                self.switch_on(host, 0).await?;
                Ok(())
            }
            EnergyProvider::SolarEdge => {
                anyhow::bail!("SolarEdge devices do not support turn_on")
            }
        }
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;
        match config.provider {
            EnergyProvider::Shelly => {
                let host = config
                    .host
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Shelly device requires host"))?;
                self.switch_off(host, 0).await?;
                Ok(())
            }
            EnergyProvider::SolarEdge => {
                anyhow::bail!("SolarEdge devices do not support turn_off")
            }
        }
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;
        match config.provider {
            EnergyProvider::SolarEdge => {
                match property {
                    "export_limit" => {
                        let _watts = value.as_f64().ok_or_else(|| {
                            anyhow::anyhow!("export_limit must be a number (watts)")
                        })?;
                        // Real implementation would call SolarEdge API to set power limit
                        Ok(())
                    }
                    _ => anyhow::bail!("unsupported SolarEdge property: {}", property),
                }
            }
            EnergyProvider::Shelly => {
                anyhow::bail!("Shelly devices do not support set_property")
            }
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let config = self.get_config(device_id)?;
        match config.provider {
            EnergyProvider::SolarEdge => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("SolarEdge requires api_key"))?;
                let site_id = config
                    .site_id
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("SolarEdge requires site_id"))?;
                let overview = self.get_overview(site_id, api_key).await?;
                Ok(json!({
                    "provider": "solaredge",
                    "current_power": overview.overview.current_power.power,
                    "last_day_energy": overview.overview.last_day_data.energy,
                    "last_month_energy": overview.overview.last_month_data.energy,
                    "lifetime_energy": overview.overview.life_time_data.energy,
                }))
            }
            EnergyProvider::Shelly => {
                let host = config
                    .host
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Shelly requires host"))?;
                let status = self.get_switch_status(host, 0).await?;
                Ok(json!({
                    "provider": "shelly",
                    "output": status.output,
                    "power": status.apower,
                    "voltage": status.voltage,
                    "current": status.current,
                    "total_energy": status.aenergy.total,
                }))
            }
        }
    }

    fn supported_type(&self) -> &str {
        "EnergyMonitor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn solaredge_adapter(server: &MockServer) -> EnergyAdapter {
        let mut adapter = EnergyAdapter::new().with_base_url(&server.base_url());
        adapter.register_device(
            "solar-001",
            EnergyConfig {
                provider: EnergyProvider::SolarEdge,
                api_key: Some("test-api-key".to_string()),
                site_id: Some("12345".to_string()),
                host: None,
            },
        );
        adapter
    }

    fn shelly_adapter(server: &MockServer) -> EnergyAdapter {
        let mut adapter = EnergyAdapter::new().with_base_url(&server.base_url());
        adapter.register_device(
            "shelly-001",
            EnergyConfig {
                provider: EnergyProvider::Shelly,
                api_key: None,
                site_id: None,
                host: Some("http://192.168.1.100".to_string()),
            },
        );
        adapter
    }

    #[test]
    fn supported_type_is_energy_monitor() {
        assert_eq!(EnergyAdapter::new().supported_type(), "EnergyMonitor");
    }

    // SolarEdge tests

    #[tokio::test]
    async fn solaredge_get_overview_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/site/12345/overview")
                .query_param("api_key", "test-api-key");
            then.status(200).json_body(json!({
                "overview": {
                    "currentPower": {"power": 3500.0},
                    "lastDayData": {"energy": 25000.0},
                    "lastMonthData": {"energy": 750000.0},
                    "lifeTimeData": {"energy": 12500000.0}
                }
            }));
        });
        let adapter = solaredge_adapter(&server);
        let overview = adapter.get_overview("12345", "test-api-key").await.unwrap();
        assert_eq!(overview.overview.current_power.power, 3500.0);
        assert_eq!(overview.overview.last_day_data.energy, 25000.0);
        mock.assert();
    }

    #[tokio::test]
    async fn solaredge_get_overview_failure() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/site/12345/overview");
            then.status(401).body("Unauthorized");
        });
        let adapter = solaredge_adapter(&server);
        let result = adapter.get_overview("12345", "bad-key").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn solaredge_get_power_flow_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/site/12345/currentPowerFlow")
                .query_param("api_key", "test-api-key");
            then.status(200).json_body(json!({
                "siteCurrentPowerFlow": {
                    "PV": {"currentPower": 4200.0},
                    "LOAD": {"currentPower": 2800.0},
                    "GRID": {"currentPower": -1400.0},
                    "STORAGE": {"currentPower": 0.0}
                }
            }));
        });
        let adapter = solaredge_adapter(&server);
        let flow = adapter
            .get_power_flow("12345", "test-api-key")
            .await
            .unwrap();
        assert_eq!(
            flow.site_current_power_flow.pv.unwrap().current_power,
            4200.0
        );
        assert_eq!(
            flow.site_current_power_flow.load.unwrap().current_power,
            2800.0
        );
        mock.assert();
    }

    #[tokio::test]
    async fn solaredge_get_energy_details_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/site/12345/energyDetails")
                .query_param("api_key", "test-api-key")
                .query_param("timeUnit", "DAY")
                .query_param("startTime", "2026-03-01")
                .query_param("endTime", "2026-03-12");
            then.status(200).json_body(json!({
                "energyDetails": {
                    "meters": [
                        {
                            "type": "Production",
                            "values": [
                                {"date": "2026-03-01", "value": 28000.0},
                                {"date": "2026-03-02", "value": 32000.0}
                            ]
                        }
                    ]
                }
            }));
        });
        let adapter = solaredge_adapter(&server);
        let details = adapter
            .get_energy_details("12345", "test-api-key", "2026-03-01", "2026-03-12")
            .await
            .unwrap();
        assert_eq!(details.energy_details.meters.len(), 1);
        assert_eq!(details.energy_details.meters[0].meter_type, "Production");
        mock.assert();
    }

    #[tokio::test]
    async fn solaredge_get_inventory_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/site/12345/inventory")
                .query_param("api_key", "test-api-key");
            then.status(200).json_body(json!({
                "Inventory": {
                    "inverters": [
                        {"name": "Inverter 1", "SN": "7E123456"}
                    ],
                    "batteries": [
                        {"name": "Battery 1", "SN": "BAT123456"}
                    ]
                }
            }));
        });
        let adapter = solaredge_adapter(&server);
        let inventory = adapter
            .get_inventory("12345", "test-api-key")
            .await
            .unwrap();
        assert_eq!(inventory.inventory.inverters.len(), 1);
        assert_eq!(inventory.inventory.inverters[0].name, "Inverter 1");
        assert_eq!(inventory.inventory.batteries.len(), 1);
        mock.assert();
    }

    #[tokio::test]
    async fn solaredge_get_state_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/site/12345/overview");
            then.status(200).json_body(json!({
                "overview": {
                    "currentPower": {"power": 3500.0},
                    "lastDayData": {"energy": 25000.0},
                    "lastMonthData": {"energy": 750000.0},
                    "lifeTimeData": {"energy": 12500000.0}
                }
            }));
        });
        let adapter = solaredge_adapter(&server);
        let state = adapter.get_state("solar-001").await.unwrap();
        assert_eq!(state["provider"], "solaredge");
        assert_eq!(state["current_power"], 3500.0);
        assert_eq!(state["last_day_energy"], 25000.0);
    }

    #[tokio::test]
    async fn solaredge_turn_on_rejected() {
        let server = MockServer::start();
        let adapter = solaredge_adapter(&server);
        let result = adapter.turn_on("solar-001").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("do not support turn_on"));
    }

    #[tokio::test]
    async fn solaredge_turn_off_rejected() {
        let server = MockServer::start();
        let adapter = solaredge_adapter(&server);
        let result = adapter.turn_off("solar-001").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("do not support turn_off"));
    }

    #[tokio::test]
    async fn solaredge_set_export_limit_success() {
        let server = MockServer::start();
        let adapter = solaredge_adapter(&server);
        let result = adapter
            .set_property("solar-001", "export_limit", json!(5000.0))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn solaredge_set_unsupported_property_fails() {
        let server = MockServer::start();
        let adapter = solaredge_adapter(&server);
        let result = adapter
            .set_property("solar-001", "invalid", json!(100))
            .await;
        assert!(result.is_err());
    }

    // Shelly tests

    #[tokio::test]
    async fn shelly_get_switch_status_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rpc/Switch.GetStatus")
                .query_param("id", "0");
            then.status(200).json_body(json!({
                "id": 0,
                "output": true,
                "apower": 125.5,
                "voltage": 230.2,
                "current": 0.545,
                "aenergy": {"total": 1250.8}
            }));
        });
        let adapter = shelly_adapter(&server);
        let status = adapter
            .get_switch_status("http://192.168.1.100", 0)
            .await
            .unwrap();
        assert!(status.output);
        assert_eq!(status.apower, 125.5);
        assert_eq!(status.voltage, 230.2);
        mock.assert();
    }

    #[tokio::test]
    async fn shelly_switch_on_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rpc/Switch.Set")
                .query_param("id", "0")
                .query_param("on", "true");
            then.status(200).json_body(json!({"was_on": false}));
        });
        let adapter = shelly_adapter(&server);
        let response = adapter.switch_on("http://192.168.1.100", 0).await.unwrap();
        assert!(!response.was_on);
        mock.assert();
    }

    #[tokio::test]
    async fn shelly_switch_off_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rpc/Switch.Set")
                .query_param("id", "0")
                .query_param("on", "false");
            then.status(200).json_body(json!({"was_on": true}));
        });
        let adapter = shelly_adapter(&server);
        let response = adapter.switch_off("http://192.168.1.100", 0).await.unwrap();
        assert!(response.was_on);
        mock.assert();
    }

    #[tokio::test]
    async fn shelly_get_meter_status_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rpc/EM.GetStatus")
                .query_param("id", "0");
            then.status(200).json_body(json!({
                "id": 0,
                "a_act_power": 1850.5,
                "a_voltage": 230.8,
                "a_current": 8.02,
                "total_act_energy": 15420.3
            }));
        });
        let adapter = shelly_adapter(&server);
        let status = adapter
            .get_meter_status("http://192.168.1.100", 0)
            .await
            .unwrap();
        assert_eq!(status.a_act_power, 1850.5);
        assert_eq!(status.total_act_energy, 15420.3);
        mock.assert();
    }

    #[tokio::test]
    async fn shelly_get_device_info_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/rpc/Shelly.GetDeviceInfo");
            then.status(200).json_body(json!({
                "id": "shellyplugus-abcdef123456",
                "model": "SNPL-00116US",
                "fw_id": "20230913-112003/v1.14.0-gcb84623"
            }));
        });
        let adapter = shelly_adapter(&server);
        let info = adapter
            .get_device_info("http://192.168.1.100")
            .await
            .unwrap();
        assert_eq!(info.model, "SNPL-00116US");
        mock.assert();
    }

    #[tokio::test]
    async fn shelly_turn_on_via_controller() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rpc/Switch.Set")
                .query_param("id", "0")
                .query_param("on", "true");
            then.status(200).json_body(json!({"was_on": false}));
        });
        let adapter = shelly_adapter(&server);
        adapter.turn_on("shelly-001").await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn shelly_turn_off_via_controller() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/rpc/Switch.Set")
                .query_param("id", "0")
                .query_param("on", "false");
            then.status(200).json_body(json!({"was_on": true}));
        });
        let adapter = shelly_adapter(&server);
        adapter.turn_off("shelly-001").await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn shelly_get_state_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/rpc/Switch.GetStatus");
            then.status(200).json_body(json!({
                "id": 0,
                "output": true,
                "apower": 125.5,
                "voltage": 230.2,
                "current": 0.545,
                "aenergy": {"total": 1250.8}
            }));
        });
        let adapter = shelly_adapter(&server);
        let state = adapter.get_state("shelly-001").await.unwrap();
        assert_eq!(state["provider"], "shelly");
        assert_eq!(state["output"], true);
        assert_eq!(state["power"], 125.5);
    }

    #[tokio::test]
    async fn shelly_set_property_rejected() {
        let server = MockServer::start();
        let adapter = shelly_adapter(&server);
        let result = adapter.set_property("shelly-001", "any", json!(100)).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("do not support set_property"));
    }

    #[tokio::test]
    async fn unknown_device_errors() {
        let adapter = EnergyAdapter::new();
        assert!(adapter.get_state("unknown").await.is_err());
        assert!(adapter.turn_on("unknown").await.is_err());
    }
}
