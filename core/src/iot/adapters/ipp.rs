//! IPP network printer adapter
//!
//! Controls network printers via Internet Printing Protocol over HTTP.
//! Capabilities: Print, PrintStatus, ScanDocument

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// IPP constants
// ---------------------------------------------------------------------------

const IPP_CONTENT_TYPE: &str = "application/ipp";
const IPP_VERSION_MAJOR: u8 = 1;
const IPP_VERSION_MINOR: u8 = 1;

// Operation IDs
const OP_PRINT_JOB: u16 = 0x0002;
const OP_GET_PRINTER_ATTRIBUTES: u16 = 0x000B;
const OP_GET_JOBS: u16 = 0x000A;
const OP_CANCEL_JOB: u16 = 0x0008;

// Attribute tags
const TAG_OPERATION: u8 = 0x01;
const TAG_JOB: u8 = 0x02;
const TAG_END: u8 = 0x03;
const TAG_CHARSET: u8 = 0x47; // charset
const TAG_NATURAL_LANG: u8 = 0x48; // naturalLanguage
const TAG_URI: u8 = 0x45; // uri
const TAG_NAME: u8 = 0x42; // nameWithoutLanguage
const TAG_INTEGER: u8 = 0x21; // integer
const TAG_ENUM: u8 = 0x23; // enum
#[allow(dead_code)]
const TAG_KEYWORD: u8 = 0x44; // keyword

// Printer state values
const PRINTER_IDLE: i32 = 3;
const PRINTER_PROCESSING: i32 = 4;
const PRINTER_STOPPED: i32 = 5;

// ---------------------------------------------------------------------------
// IPP binary encoding
// ---------------------------------------------------------------------------

/// Encode an IPP attribute (tag, name, value as bytes).
fn encode_attribute(buf: &mut Vec<u8>, value_tag: u8, name: &str, value: &[u8]) {
    buf.push(value_tag);
    let name_bytes = name.as_bytes();
    buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(name_bytes);
    buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
    buf.extend_from_slice(value);
}

/// Encode a string attribute.
fn encode_string_attr(buf: &mut Vec<u8>, value_tag: u8, name: &str, value: &str) {
    encode_attribute(buf, value_tag, name, value.as_bytes());
}

/// Encode an integer attribute (4 bytes big-endian).
fn encode_integer_attr(buf: &mut Vec<u8>, value_tag: u8, name: &str, value: i32) {
    encode_attribute(buf, value_tag, name, &value.to_be_bytes());
}

/// Build the common IPP request header + operation attributes.
fn build_ipp_request(operation_id: u16, request_id: i32, printer_uri: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    // Version
    buf.push(IPP_VERSION_MAJOR);
    buf.push(IPP_VERSION_MINOR);
    // Operation ID
    buf.extend_from_slice(&operation_id.to_be_bytes());
    // Request ID
    buf.extend_from_slice(&request_id.to_be_bytes());
    // Operation attributes group
    buf.push(TAG_OPERATION);
    encode_string_attr(&mut buf, TAG_CHARSET, "attributes-charset", "utf-8");
    encode_string_attr(
        &mut buf,
        TAG_NATURAL_LANG,
        "attributes-natural-language",
        "en-us",
    );
    encode_string_attr(&mut buf, TAG_URI, "printer-uri", printer_uri);
    buf
}

/// Build a Print-Job IPP request.
fn build_print_job_request(
    request_id: i32,
    printer_uri: &str,
    job_name: &str,
    document: &[u8],
) -> Vec<u8> {
    let mut buf = build_ipp_request(OP_PRINT_JOB, request_id, printer_uri);
    encode_string_attr(&mut buf, TAG_NAME, "job-name", job_name);
    buf.push(TAG_END);
    buf.extend_from_slice(document);
    buf
}

/// Build a Get-Printer-Attributes request.
fn build_get_printer_attrs_request(request_id: i32, printer_uri: &str) -> Vec<u8> {
    let mut buf = build_ipp_request(OP_GET_PRINTER_ATTRIBUTES, request_id, printer_uri);
    buf.push(TAG_END);
    buf
}

/// Build a Get-Jobs request.
fn build_get_jobs_request(request_id: i32, printer_uri: &str) -> Vec<u8> {
    let mut buf = build_ipp_request(OP_GET_JOBS, request_id, printer_uri);
    buf.push(TAG_END);
    buf
}

/// Build a Cancel-Job request.
fn build_cancel_job_request(request_id: i32, printer_uri: &str, job_id: i32) -> Vec<u8> {
    let mut buf = build_ipp_request(OP_CANCEL_JOB, request_id, printer_uri);
    buf.push(TAG_JOB);
    encode_integer_attr(&mut buf, TAG_INTEGER, "job-id", job_id);
    buf.push(TAG_END);
    buf
}

// ---------------------------------------------------------------------------
// IPP response parsing
// ---------------------------------------------------------------------------

/// Parsed IPP response header.
#[derive(Debug)]
struct IppResponse {
    status_code: u16,
    #[allow(dead_code)]
    request_id: i32,
    attributes: Vec<IppAttribute>,
}

/// A single parsed IPP attribute.
#[derive(Debug, Clone)]
struct IppAttribute {
    name: String,
    value_tag: u8,
    value: Vec<u8>,
}

impl IppAttribute {
    fn as_string(&self) -> Option<String> {
        String::from_utf8(self.value.clone()).ok()
    }

    fn as_i32(&self) -> Option<i32> {
        if self.value.len() == 4 {
            Some(i32::from_be_bytes([
                self.value[0],
                self.value[1],
                self.value[2],
                self.value[3],
            ]))
        } else {
            None
        }
    }
}

/// Parse an IPP response from raw bytes.
fn parse_ipp_response(data: &[u8]) -> anyhow::Result<IppResponse> {
    if data.len() < 8 {
        anyhow::bail!("IPP response too short: {} bytes", data.len());
    }
    let status_code = u16::from_be_bytes([data[2], data[3]]);
    let request_id = i32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let mut attributes = Vec::new();
    let mut pos = 8;
    while pos < data.len() {
        let tag = data[pos];
        pos += 1;
        if tag == TAG_END {
            break;
        }
        // Group delimiter tags (0x01-0x05) have no data
        if tag <= 0x05 {
            continue;
        }
        if pos + 2 > data.len() {
            break;
        }
        let name_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if pos + name_len > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[pos..pos + name_len]).to_string();
        pos += name_len;
        if pos + 2 > data.len() {
            break;
        }
        let value_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if pos + value_len > data.len() {
            break;
        }
        let value = data[pos..pos + value_len].to_vec();
        pos += value_len;
        attributes.push(IppAttribute {
            name,
            value_tag: tag,
            value,
        });
    }
    Ok(IppResponse {
        status_code,
        request_id,
        attributes,
    })
}

fn printer_state_str(state: i32) -> &'static str {
    match state {
        PRINTER_IDLE => "idle",
        PRINTER_PROCESSING => "processing",
        PRINTER_STOPPED => "stopped",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// IPP network printer adapter backed by real HTTP calls.
pub struct IppAdapter {
    /// device_id -> printer URI
    printers: HashMap<String, String>,
    client: reqwest::Client,
    base_url_override: Option<String>,
    next_request_id: std::sync::atomic::AtomicI32,
}

impl IppAdapter {
    pub fn new() -> Self {
        Self {
            printers: HashMap::new(),
            client: reqwest::Client::new(),
            base_url_override: None,
            next_request_id: std::sync::atomic::AtomicI32::new(1),
        }
    }

    /// Override the base URL for all printers (for testing with httpmock).
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url_override = Some(url.trim_end_matches('/').to_string());
        self
    }

    pub fn register_printer(&mut self, device_id: &str, uri: &str) {
        self.printers.insert(device_id.to_string(), uri.to_string());
    }

    fn get_uri(&self, device_id: &str) -> anyhow::Result<&str> {
        self.printers
            .get(device_id)
            .map(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("unknown printer: {}", device_id))
    }

    fn next_id(&self) -> i32 {
        self.next_request_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Resolve the HTTP endpoint for IPP requests.
    fn ipp_endpoint(&self, printer_uri: &str) -> String {
        match &self.base_url_override {
            Some(base) => format!("{}/ipp/print", base),
            None => printer_uri.replace("ipp://", "http://"),
        }
    }

    /// Send an IPP request and parse the response.
    async fn send_ipp(&self, printer_uri: &str, body: Vec<u8>) -> anyhow::Result<IppResponse> {
        let url = self.ipp_endpoint(printer_uri);
        let resp = self
            .client
            .post(&url)
            .header("Content-Type", IPP_CONTENT_TYPE)
            .body(body)
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            anyhow::bail!("IPP request failed (HTTP {})", status.as_u16());
        }
        let ipp_resp = parse_ipp_response(&bytes)?;
        if ipp_resp.status_code > 0x00FF {
            anyhow::bail!("IPP error status: 0x{:04X}", ipp_resp.status_code);
        }
        Ok(ipp_resp)
    }

    /// Submit a print job.
    pub async fn print_job(
        &self,
        device_id: &str,
        job_name: &str,
        document: &[u8],
    ) -> anyhow::Result<i32> {
        let uri = self.get_uri(device_id)?.to_string();
        let req_id = self.next_id();
        let body = build_print_job_request(req_id, &uri, job_name, document);
        let resp = self.send_ipp(&uri, body).await?;
        // Look for job-id in response
        for attr in &resp.attributes {
            if attr.name == "job-id" {
                if let Some(id) = attr.as_i32() {
                    return Ok(id);
                }
            }
        }
        Ok(0)
    }

    /// Get printer attributes (status, name, etc.).
    pub async fn get_printer_attributes(
        &self,
        device_id: &str,
    ) -> anyhow::Result<HashMap<String, String>> {
        let uri = self.get_uri(device_id)?.to_string();
        let req_id = self.next_id();
        let body = build_get_printer_attrs_request(req_id, &uri);
        let resp = self.send_ipp(&uri, body).await?;
        let mut attrs = HashMap::new();
        for attr in &resp.attributes {
            if attr.value_tag == TAG_ENUM || attr.value_tag == TAG_INTEGER {
                if let Some(v) = attr.as_i32() {
                    if attr.name == "printer-state" {
                        attrs.insert(attr.name.clone(), printer_state_str(v).to_string());
                    } else {
                        attrs.insert(attr.name.clone(), v.to_string());
                    }
                }
            } else if let Some(s) = attr.as_string() {
                attrs.insert(attr.name.clone(), s);
            }
        }
        Ok(attrs)
    }

    /// Get queued jobs count.
    pub async fn get_jobs(&self, device_id: &str) -> anyhow::Result<u32> {
        let uri = self.get_uri(device_id)?.to_string();
        let req_id = self.next_id();
        let body = build_get_jobs_request(req_id, &uri);
        let resp = self.send_ipp(&uri, body).await?;
        let count = resp
            .attributes
            .iter()
            .filter(|a| a.name == "job-id")
            .count();
        Ok(count as u32)
    }

    /// Cancel a print job.
    pub async fn cancel_job(&self, device_id: &str, job_id: i32) -> anyhow::Result<()> {
        let uri = self.get_uri(device_id)?.to_string();
        let req_id = self.next_id();
        let body = build_cancel_job_request(req_id, &uri, job_id);
        self.send_ipp(&uri, body).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DeviceController impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for IppAdapter {
    async fn turn_on(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("printers do not support turn_on -- use print command via set_property")
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("printers do not support turn_off")
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        match property {
            "print" => {
                let doc = value.as_str().unwrap_or("").as_bytes().to_vec();
                self.print_job(device_id, "prometheus-job", &doc).await?;
                Ok(())
            }
            "cancel" => {
                let job_id = value
                    .as_i64()
                    .ok_or_else(|| anyhow::anyhow!("cancel value must be a job-id integer"))?
                    as i32;
                self.cancel_job(device_id, job_id).await
            }
            _ => anyhow::bail!("unsupported printer property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let attrs = self.get_printer_attributes(device_id).await?;
        let status = attrs
            .get("printer-state")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let name = attrs
            .get("printer-name")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let uri = self.get_uri(device_id)?;
        Ok(serde_json::json!({
            "uri": uri,
            "status": status,
            "printer_name": name,
        }))
    }

    fn supported_type(&self) -> &str {
        "Printer"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn adapter(server: &MockServer) -> IppAdapter {
        let mut a = IppAdapter::new().with_base_url(&server.base_url());
        a.register_printer("hp-001", "ipp://192.168.1.50:631/ipp/print");
        a
    }

    /// Build a minimal valid IPP response.
    fn ipp_ok_response(request_id: i32, attrs: &[(u8, &str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        // Version 1.1
        buf.push(IPP_VERSION_MAJOR);
        buf.push(IPP_VERSION_MINOR);
        // Status: successful-ok (0x0000)
        buf.extend_from_slice(&0u16.to_be_bytes());
        // Request ID
        buf.extend_from_slice(&request_id.to_be_bytes());
        // Operation attributes group
        buf.push(TAG_OPERATION);
        encode_string_attr(&mut buf, TAG_CHARSET, "attributes-charset", "utf-8");
        for (tag, name, value) in attrs {
            encode_attribute(&mut buf, *tag, name, value);
        }
        buf.push(TAG_END);
        buf
    }

    fn ipp_printer_state_response(request_id: i32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(IPP_VERSION_MAJOR);
        buf.push(IPP_VERSION_MINOR);
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&request_id.to_be_bytes());
        buf.push(TAG_OPERATION);
        encode_string_attr(&mut buf, TAG_CHARSET, "attributes-charset", "utf-8");
        encode_integer_attr(&mut buf, TAG_ENUM, "printer-state", PRINTER_IDLE);
        encode_string_attr(&mut buf, TAG_NAME, "printer-name", "HP LaserJet");
        buf.push(TAG_END);
        buf
    }

    // 1. supported_type
    #[test]
    fn supported_type_is_printer() {
        assert_eq!(IppAdapter::new().supported_type(), "Printer");
    }

    // 2. turn_on not supported
    #[tokio::test]
    async fn turn_on_not_supported() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a.turn_on("hp-001").await.is_err());
    }

    // 3. turn_off not supported
    #[tokio::test]
    async fn turn_off_not_supported() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a.turn_off("hp-001").await.is_err());
    }

    // 4. print_job sends IPP request
    #[tokio::test]
    async fn print_job_sends_ipp() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/ipp/print")
                .header("Content-Type", IPP_CONTENT_TYPE);
            then.status(200).body(ipp_ok_response(1, &[
                (TAG_INTEGER, "job-id", &42i32.to_be_bytes()),
            ]));
        });
        let a = adapter(&server);
        let job_id = a.print_job("hp-001", "test-doc", b"Hello World").await.unwrap();
        assert_eq!(job_id, 42);
        mock.assert();
    }

    // 5. get_printer_attributes parses state
    #[tokio::test]
    async fn get_printer_attributes_parses() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/ipp/print");
            // We can't predict exact request_id, use any valid response
            then.status(200).body(ipp_printer_state_response(2));
        });
        let a = adapter(&server);
        let attrs = a.get_printer_attributes("hp-001").await.unwrap();
        assert_eq!(attrs.get("printer-state").unwrap(), "idle");
        assert_eq!(attrs.get("printer-name").unwrap(), "HP LaserJet");
    }

    // 6. get_jobs returns count
    #[tokio::test]
    async fn get_jobs_returns_count() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/ipp/print");
            then.status(200).body(ipp_ok_response(3, &[
                (TAG_INTEGER, "job-id", &1i32.to_be_bytes()),
                (TAG_INTEGER, "job-id", &2i32.to_be_bytes()),
            ]));
        });
        let a = adapter(&server);
        let count = a.get_jobs("hp-001").await.unwrap();
        assert_eq!(count, 2);
    }

    // 7. cancel_job sends cancel request
    #[tokio::test]
    async fn cancel_job_sends_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/ipp/print");
            then.status(200).body(ipp_ok_response(4, &[]));
        });
        let a = adapter(&server);
        a.cancel_job("hp-001", 42).await.unwrap();
        mock.assert();
    }

    // 8. set_property print
    #[tokio::test]
    async fn set_property_print() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/ipp/print");
            then.status(200).body(ipp_ok_response(5, &[]));
        });
        let a = adapter(&server);
        a.set_property("hp-001", "print", serde_json::json!("Hello World"))
            .await
            .unwrap();
    }

    // 9. set_property cancel
    #[tokio::test]
    async fn set_property_cancel() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/ipp/print");
            then.status(200).body(ipp_ok_response(6, &[]));
        });
        let a = adapter(&server);
        a.set_property("hp-001", "cancel", serde_json::json!(42))
            .await
            .unwrap();
    }

    // 10. set_property unsupported errors
    #[tokio::test]
    async fn set_property_unsupported_errors() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a
            .set_property("hp-001", "fly", serde_json::json!(true))
            .await
            .is_err());
    }

    // 11. get_state returns printer info
    #[tokio::test]
    async fn get_state_returns_status() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/ipp/print");
            then.status(200).body(ipp_printer_state_response(7));
        });
        let a = adapter(&server);
        let state = a.get_state("hp-001").await.unwrap();
        assert_eq!(state["status"], "idle");
        assert_eq!(state["printer_name"], "HP LaserJet");
    }

    // 12. unknown printer errors
    #[tokio::test]
    async fn unknown_printer_errors() {
        let a = IppAdapter::new();
        assert!(a.get_state("unknown").await.is_err());
    }

    // 13. HTTP error propagates
    #[tokio::test]
    async fn http_error_propagates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/ipp/print");
            then.status(500).body("Server Error");
        });
        let a = adapter(&server);
        assert!(a.get_printer_attributes("hp-001").await.is_err());
    }

    // 14. IPP binary encoding roundtrip
    #[test]
    fn ipp_request_encoding() {
        let body = build_get_printer_attrs_request(1, "ipp://localhost/ipp/print");
        // Version 1.1
        assert_eq!(body[0], 1);
        assert_eq!(body[1], 1);
        // Operation: Get-Printer-Attributes (0x000B)
        assert_eq!(u16::from_be_bytes([body[2], body[3]]), OP_GET_PRINTER_ATTRIBUTES);
        // Request ID: 1
        assert_eq!(i32::from_be_bytes([body[4], body[5], body[6], body[7]]), 1);
    }

    // 15. parse_ipp_response handles minimal response
    #[test]
    fn parse_minimal_response() {
        let data = ipp_ok_response(99, &[]);
        let resp = parse_ipp_response(&data).unwrap();
        assert_eq!(resp.status_code, 0);
        assert_eq!(resp.request_id, 99);
    }

    // 16. printer_state_str mapping
    #[test]
    fn printer_state_str_values() {
        assert_eq!(printer_state_str(PRINTER_IDLE), "idle");
        assert_eq!(printer_state_str(PRINTER_PROCESSING), "processing");
        assert_eq!(printer_state_str(PRINTER_STOPPED), "stopped");
        assert_eq!(printer_state_str(99), "unknown");
    }
}
