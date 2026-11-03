//! 3D Printer adapter for OctoPrint
//!
//! Controls 3D printers via OctoPrint REST API.
//! Capabilities: PrintControl, TemperatureControl, ProgressMonitoring

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// Config & types
// ---------------------------------------------------------------------------

/// OctoPrint configuration
#[derive(Debug, Clone)]
pub struct OctoPrintConfig {
    pub api_key: String,
    pub base_url: String,
}

/// Printer state from /api/printer
#[derive(Debug, Deserialize)]
pub struct PrinterState {
    temperature: Temperature,
    state: StateFlags,
}

#[derive(Debug, Deserialize)]
struct Temperature {
    tool0: Option<ToolTemp>,
    bed: Option<BedTemp>,
}

#[derive(Debug, Deserialize)]
struct ToolTemp {
    actual: f64,
    target: f64,
}

#[derive(Debug, Deserialize)]
struct BedTemp {
    actual: f64,
    target: f64,
}

#[derive(Debug, Deserialize)]
struct StateFlags {
    text: String,
    flags: Flags,
}

#[derive(Debug, Deserialize)]
struct Flags {
    operational: bool,
    printing: bool,
    paused: bool,
    ready: bool,
    error: bool,
}

/// Job information from /api/job
#[derive(Debug, Deserialize)]
pub struct JobInfo {
    job: Job,
    progress: Progress,
    state: String,
}

#[derive(Debug, Deserialize)]
struct Job {
    file: FileInfo,
}

#[derive(Debug, Deserialize)]
struct FileInfo {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Progress {
    completion: Option<f64>,
    #[serde(rename = "printTime")]
    print_time: Option<i64>,
    #[serde(rename = "printTimeLeft")]
    print_time_left: Option<i64>,
}

/// Connection status from /api/connection
#[derive(Debug, Deserialize)]
pub struct ConnectionStatus {
    #[allow(dead_code)]
    current: ConnectionState,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ConnectionState {
    state: String,
}

/// Generic OctoPrint API response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OctoPrintResponse {
    success: Option<bool>,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// OctoPrint adapter backed by real HTTP calls
pub struct OctoPrintAdapter {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    printers: HashMap<String, String>, // device_id -> printer_name
}

impl OctoPrintAdapter {
    /// Create a new adapter with the given config
    pub fn new(config: OctoPrintConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key,
            printers: HashMap::new(),
        }
    }

    /// Override the base URL (for testing with httpmock)
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    /// Register a printer device
    pub fn register_printer(&mut self, device_id: &str, name: &str) {
        self.printers
            .insert(device_id.to_string(), name.to_string());
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    fn get_printer_name(&self, device_id: &str) -> anyhow::Result<&str> {
        self.printers
            .get(device_id)
            .map(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("unknown printer: {}", device_id))
    }

    /// GET /api/printer - get printer state
    pub async fn get_printer_state(&self, device_id: &str) -> anyhow::Result<PrinterState> {
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/printer");
        let resp = self
            .client
            .get(&url)
            .header("X-Api-Key", &self.api_key)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        let state: PrinterState = resp.json().await?;
        Ok(state)
    }

    /// GET /api/job - get current job info
    pub async fn get_job(&self, device_id: &str) -> anyhow::Result<JobInfo> {
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/job");
        let resp = self
            .client
            .get(&url)
            .header("X-Api-Key", &self.api_key)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        let job: JobInfo = resp.json().await?;
        Ok(job)
    }

    /// POST /api/job - start job
    pub async fn start_job(&self, device_id: &str) -> anyhow::Result<()> {
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/job");
        let body = json!({"command": "start"});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    /// POST /api/job - cancel job
    pub async fn cancel_job(&self, device_id: &str) -> anyhow::Result<()> {
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/job");
        let body = json!({"command": "cancel"});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    /// POST /api/job - pause job
    pub async fn pause_job(&self, device_id: &str) -> anyhow::Result<()> {
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/job");
        let body = json!({"command": "pause", "action": "pause"});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    /// POST /api/job - resume job
    pub async fn resume_job(&self, device_id: &str) -> anyhow::Result<()> {
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/job");
        let body = json!({"command": "pause", "action": "resume"});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    /// POST /api/printer/tool - set extruder temperature
    pub async fn set_tool_temp(&self, device_id: &str, temp: f64) -> anyhow::Result<()> {
        self.get_printer_name(device_id)?;
        if temp < 0.0 || temp > 300.0 {
            anyhow::bail!("tool temperature must be 0-300°C");
        }
        let url = self.endpoint("/api/printer/tool");
        let body = json!({"command": "target", "targets": {"tool0": temp}});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    /// POST /api/printer/bed - set bed temperature
    pub async fn set_bed_temp(&self, device_id: &str, temp: f64) -> anyhow::Result<()> {
        self.get_printer_name(device_id)?;
        if temp < 0.0 || temp > 120.0 {
            anyhow::bail!("bed temperature must be 0-120°C");
        }
        let url = self.endpoint("/api/printer/bed");
        let body = json!({"command": "target", "target": temp});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    /// POST /api/printer/command - send GCode commands
    pub async fn send_gcode(&self, device_id: &str, commands: Vec<String>) -> anyhow::Result<()> {
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/printer/command");
        let body = json!({"commands": commands});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    /// GET /api/connection - get connection status
    pub async fn get_connection(&self, device_id: &str) -> anyhow::Result<ConnectionStatus> {
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/connection");
        let resp = self
            .client
            .get(&url)
            .header("X-Api-Key", &self.api_key)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        let status: ConnectionStatus = resp.json().await?;
        Ok(status)
    }
}

#[async_trait]
impl DeviceController for OctoPrintAdapter {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        // Connect to printer
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/connection");
        let body = json!({"command": "connect"});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        // Disconnect from printer
        self.get_printer_name(device_id)?;
        let url = self.endpoint("/api/connection");
        let body = json!({"command": "disconnect"});
        let resp = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OctoPrint API error: {}", resp.status());
        }

        Ok(())
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        match property {
            "start" => self.start_job(device_id).await,
            "cancel" => self.cancel_job(device_id).await,
            "pause" => self.pause_job(device_id).await,
            "resume" => self.resume_job(device_id).await,
            "tool_temp" => {
                let temp = value
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("tool_temp must be a number"))?;
                self.set_tool_temp(device_id, temp).await
            }
            "bed_temp" => {
                let temp = value
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("bed_temp must be a number"))?;
                self.set_bed_temp(device_id, temp).await
            }
            "gcode" => {
                let commands = if let Some(s) = value.as_str() {
                    vec![s.to_string()]
                } else if let Some(arr) = value.as_array() {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                } else {
                    anyhow::bail!("gcode must be a string or array of strings");
                };
                self.send_gcode(device_id, commands).await
            }
            _ => anyhow::bail!("unsupported property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let printer_state = self.get_printer_state(device_id).await?;
        let job_info = self.get_job(device_id).await?;

        Ok(json!({
            "state": printer_state.state.text,
            "operational": printer_state.state.flags.operational,
            "printing": printer_state.state.flags.printing,
            "paused": printer_state.state.flags.paused,
            "ready": printer_state.state.flags.ready,
            "error": printer_state.state.flags.error,
            "tool_temp": printer_state.temperature.tool0.as_ref().map(|t| t.actual),
            "tool_target": printer_state.temperature.tool0.as_ref().map(|t| t.target),
            "bed_temp": printer_state.temperature.bed.as_ref().map(|t| t.actual),
            "bed_target": printer_state.temperature.bed.as_ref().map(|t| t.target),
            "job_state": job_info.state,
            "file_name": job_info.job.file.name,
            "progress": job_info.progress.completion,
            "print_time": job_info.progress.print_time,
            "print_time_left": job_info.progress.print_time_left,
        }))
    }

    fn supported_type(&self) -> &str {
        "Printer3d"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn test_config() -> OctoPrintConfig {
        OctoPrintConfig {
            api_key: "test-api-key-12345".to_string(),
            base_url: "http://octopi.local".to_string(),
        }
    }

    fn setup(server: &MockServer) -> OctoPrintAdapter {
        let mut adapter = OctoPrintAdapter::new(test_config()).with_base_url(&server.base_url());
        adapter.register_printer("printer-001", "Ender 3");
        adapter
    }

    // 1. supported_type returns Printer3d
    #[test]
    fn supported_type_is_printer3d() {
        let adapter = OctoPrintAdapter::new(test_config());
        assert_eq!(adapter.supported_type(), "Printer3d");
    }

    // 2. register_printer stores device
    #[test]
    fn register_printer_stores_device() {
        let mut adapter = OctoPrintAdapter::new(test_config());
        adapter.register_printer("p1", "My Printer");
        assert!(adapter.get_printer_name("p1").is_ok());
        assert_eq!(adapter.get_printer_name("p1").unwrap(), "My Printer");
    }

    // 3. get_printer_state success
    #[tokio::test]
    async fn get_printer_state_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/api/printer")
                .header("X-Api-Key", "test-api-key-12345");
            then.status(200).json_body(json!({
                "temperature": {
                    "tool0": {"actual": 25.5, "target": 0.0},
                    "bed": {"actual": 24.0, "target": 0.0}
                },
                "state": {
                    "text": "Operational",
                    "flags": {
                        "operational": true,
                        "printing": false,
                        "paused": false,
                        "ready": true,
                        "error": false
                    }
                }
            }));
        });
        let adapter = setup(&server);
        let state = adapter.get_printer_state("printer-001").await.unwrap();
        assert_eq!(state.state.text, "Operational");
        assert!(state.state.flags.operational);
        assert!(!state.state.flags.printing);
    }

    // 4. get_job success
    #[tokio::test]
    async fn get_job_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/api/job")
                .header("X-Api-Key", "test-api-key-12345");
            then.status(200).json_body(json!({
                "job": {
                    "file": {"name": "test.gcode"}
                },
                "progress": {
                    "completion": 45.5,
                    "printTime": 1200,
                    "printTimeLeft": 1400
                },
                "state": "Printing"
            }));
        });
        let adapter = setup(&server);
        let job = adapter.get_job("printer-001").await.unwrap();
        assert_eq!(job.state, "Printing");
        assert_eq!(job.job.file.name, Some("test.gcode".to_string()));
        assert_eq!(job.progress.completion, Some(45.5));
    }

    // 5. start_job success
    #[tokio::test]
    async fn start_job_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/job")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"command": "start"}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter.start_job("printer-001").await.is_ok());
    }

    // 6. cancel_job success
    #[tokio::test]
    async fn cancel_job_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/job")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"command": "cancel"}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter.cancel_job("printer-001").await.is_ok());
    }

    // 7. pause_job success
    #[tokio::test]
    async fn pause_job_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/job")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"command": "pause", "action": "pause"}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter.pause_job("printer-001").await.is_ok());
    }

    // 8. resume_job success
    #[tokio::test]
    async fn resume_job_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/job")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"command": "pause", "action": "resume"}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter.resume_job("printer-001").await.is_ok());
    }

    // 9. set_tool_temp success
    #[tokio::test]
    async fn set_tool_temp_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/printer/tool")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"command": "target", "targets": {"tool0": 200.0}}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter.set_tool_temp("printer-001", 200.0).await.is_ok());
    }

    // 10. set_tool_temp validates range
    #[tokio::test]
    async fn set_tool_temp_validates_range() {
        let server = MockServer::start();
        let adapter = setup(&server);
        assert!(adapter.set_tool_temp("printer-001", -10.0).await.is_err());
        assert!(adapter.set_tool_temp("printer-001", 350.0).await.is_err());
    }

    // 11. set_bed_temp success
    #[tokio::test]
    async fn set_bed_temp_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/printer/bed")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"command": "target", "target": 60.0}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter.set_bed_temp("printer-001", 60.0).await.is_ok());
    }

    // 12. set_bed_temp validates range
    #[tokio::test]
    async fn set_bed_temp_validates_range() {
        let server = MockServer::start();
        let adapter = setup(&server);
        assert!(adapter.set_bed_temp("printer-001", -5.0).await.is_err());
        assert!(adapter.set_bed_temp("printer-001", 150.0).await.is_err());
    }

    // 13. send_gcode success
    #[tokio::test]
    async fn send_gcode_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/printer/command")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"commands": ["G28", "G1 X10 Y10"]}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter
            .send_gcode(
                "printer-001",
                vec!["G28".to_string(), "G1 X10 Y10".to_string()]
            )
            .await
            .is_ok());
    }

    // 14. get_connection success
    #[tokio::test]
    async fn get_connection_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/api/connection")
                .header("X-Api-Key", "test-api-key-12345");
            then.status(200).json_body(json!({
                "current": {
                    "state": "Operational"
                }
            }));
        });
        let adapter = setup(&server);
        let conn = adapter.get_connection("printer-001").await.unwrap();
        assert_eq!(conn.current.state, "Operational");
    }

    // 15. turn_on connects printer
    #[tokio::test]
    async fn turn_on_connects_printer() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/connection")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"command": "connect"}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter.turn_on("printer-001").await.is_ok());
    }

    // 16. turn_off disconnects printer
    #[tokio::test]
    async fn turn_off_disconnects_printer() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/connection")
                .header("X-Api-Key", "test-api-key-12345")
                .json_body(json!({"command": "disconnect"}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter.turn_off("printer-001").await.is_ok());
    }

    // 17. set_property routes to correct commands
    #[tokio::test]
    async fn set_property_start() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/job");
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter
            .set_property("printer-001", "start", json!(null))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn set_property_cancel() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/job");
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter
            .set_property("printer-001", "cancel", json!(null))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn set_property_tool_temp() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/printer/tool");
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter
            .set_property("printer-001", "tool_temp", json!(210.0))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn set_property_bed_temp() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/printer/bed");
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter
            .set_property("printer-001", "bed_temp", json!(70.0))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn set_property_gcode_string() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/printer/command")
                .json_body(json!({"commands": ["G28"]}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter
            .set_property("printer-001", "gcode", json!("G28"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn set_property_gcode_array() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/printer/command")
                .json_body(json!({"commands": ["G28", "M104 S200"]}));
            then.status(204);
        });
        let adapter = setup(&server);
        assert!(adapter
            .set_property("printer-001", "gcode", json!(["G28", "M104 S200"]))
            .await
            .is_ok());
    }

    // 18. get_state returns combined printer and job info
    #[tokio::test]
    async fn get_state_returns_combined_info() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/printer");
            then.status(200).json_body(json!({
                "temperature": {
                    "tool0": {"actual": 210.0, "target": 210.0},
                    "bed": {"actual": 60.0, "target": 60.0}
                },
                "state": {
                    "text": "Printing",
                    "flags": {
                        "operational": true,
                        "printing": true,
                        "paused": false,
                        "ready": false,
                        "error": false
                    }
                }
            }));
        });
        server.mock(|when, then| {
            when.method(GET).path("/api/job");
            then.status(200).json_body(json!({
                "job": {
                    "file": {"name": "benchy.gcode"}
                },
                "progress": {
                    "completion": 75.2,
                    "printTime": 3600,
                    "printTimeLeft": 1200
                },
                "state": "Printing"
            }));
        });
        let adapter = setup(&server);
        let state = adapter.get_state("printer-001").await.unwrap();
        assert_eq!(state["state"], "Printing");
        assert_eq!(state["printing"], true);
        assert_eq!(state["tool_temp"], 210.0);
        assert_eq!(state["bed_temp"], 60.0);
        assert_eq!(state["file_name"], "benchy.gcode");
        assert_eq!(state["progress"], 75.2);
    }

    // 19. unknown device errors
    #[tokio::test]
    async fn unknown_device_errors() {
        let server = MockServer::start();
        let adapter = OctoPrintAdapter::new(test_config()).with_base_url(&server.base_url());
        assert!(adapter.get_printer_state("unknown").await.is_err());
        assert!(adapter.start_job("unknown").await.is_err());
        assert!(adapter.get_state("unknown").await.is_err());
    }

    // 20. HTTP error propagates
    #[tokio::test]
    async fn http_error_propagates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/printer");
            then.status(500).body("Internal Server Error");
        });
        let adapter = setup(&server);
        assert!(adapter.get_printer_state("printer-001").await.is_err());
    }

    // 21. unsupported property errors
    #[tokio::test]
    async fn unsupported_property_errors() {
        let server = MockServer::start();
        let adapter = setup(&server);
        let result = adapter
            .set_property("printer-001", "invalid_prop", json!(123))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsupported property"));
    }

    // 22. with_base_url trims trailing slash
    #[test]
    fn with_base_url_trims_trailing_slash() {
        let adapter = OctoPrintAdapter::new(test_config()).with_base_url("http://localhost:5000/");
        assert_eq!(adapter.base_url, "http://localhost:5000");
    }

    // 23. endpoint builds correct URL
    #[test]
    fn endpoint_builds_correct_url() {
        let adapter = OctoPrintAdapter::new(test_config()).with_base_url("http://localhost:5000");
        let url = adapter.endpoint("/api/printer");
        assert_eq!(url, "http://localhost:5000/api/printer");
    }
}
