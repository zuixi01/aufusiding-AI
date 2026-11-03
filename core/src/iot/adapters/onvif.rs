//! ONVIF IP camera adapter
//!
//! Controls IP cameras via ONVIF SOAP over HTTP.
//! Capabilities: VideoStream, VideoRecord, MotionDetect, PTZ

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// ONVIF camera configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnvifConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub rtsp_port: u16,
}

// ---------------------------------------------------------------------------
// SOAP helpers
// ---------------------------------------------------------------------------

const SOAP_ENVELOPE_OPEN: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"
            xmlns:trt="http://www.onvif.org/ver10/media/wsdl"
            xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl"
            xmlns:tt="http://www.onvif.org/ver10/schema">
  <s:Body>"#;

const SOAP_ENVELOPE_CLOSE: &str = r#"
  </s:Body>
</s:Envelope>"#;

fn soap_get_profiles() -> String {
    format!(
        "{}\n    <trt:GetProfiles/>{}",
        SOAP_ENVELOPE_OPEN, SOAP_ENVELOPE_CLOSE
    )
}

fn soap_get_stream_uri(profile_token: &str) -> String {
    format!(
        "{}\n    <trt:GetStreamUri>\
         \n      <trt:StreamSetup>\
         \n        <tt:Stream>RTP-Unicast</tt:Stream>\
         \n        <tt:Transport><tt:Protocol>RTSP</tt:Protocol></tt:Transport>\
         \n      </trt:StreamSetup>\
         \n      <trt:ProfileToken>{}</trt:ProfileToken>\
         \n    </trt:GetStreamUri>{}",
        SOAP_ENVELOPE_OPEN, profile_token, SOAP_ENVELOPE_CLOSE
    )
}

fn soap_get_snapshot_uri(profile_token: &str) -> String {
    format!(
        "{}\n    <trt:GetSnapshotUri>\
         \n      <trt:ProfileToken>{}</trt:ProfileToken>\
         \n    </trt:GetSnapshotUri>{}",
        SOAP_ENVELOPE_OPEN, profile_token, SOAP_ENVELOPE_CLOSE
    )
}

fn soap_continuous_move(profile_token: &str, pan: f64, tilt: f64, zoom: f64) -> String {
    format!(
        "{}\n    <tptz:ContinuousMove>\
         \n      <tptz:ProfileToken>{}</tptz:ProfileToken>\
         \n      <tptz:Velocity>\
         \n        <tt:PanTilt x=\"{}\" y=\"{}\"/>\
         \n        <tt:Zoom x=\"{}\"/>\
         \n      </tptz:Velocity>\
         \n    </tptz:ContinuousMove>{}",
        SOAP_ENVELOPE_OPEN, profile_token, pan, tilt, zoom, SOAP_ENVELOPE_CLOSE
    )
}

fn soap_stop(profile_token: &str) -> String {
    format!(
        "{}\n    <tptz:Stop>\
         \n      <tptz:ProfileToken>{}</tptz:ProfileToken>\
         \n      <tptz:PanTilt>true</tptz:PanTilt>\
         \n      <tptz:Zoom>true</tptz:Zoom>\
         \n    </tptz:Stop>{}",
        SOAP_ENVELOPE_OPEN, profile_token, SOAP_ENVELOPE_CLOSE
    )
}

fn soap_absolute_move(profile_token: &str, pan: f64, tilt: f64, zoom: f64) -> String {
    format!(
        "{}\n    <tptz:AbsoluteMove>\
         \n      <tptz:ProfileToken>{}</tptz:ProfileToken>\
         \n      <tptz:Position>\
         \n        <tt:PanTilt x=\"{}\" y=\"{}\"/>\
         \n        <tt:Zoom x=\"{}\"/>\
         \n      </tptz:Position>\
         \n    </tptz:AbsoluteMove>{}",
        SOAP_ENVELOPE_OPEN, profile_token, pan, tilt, zoom, SOAP_ENVELOPE_CLOSE
    )
}

/// Extract text content between XML tags (simple, no nested same-name tags).
/// Handles both bare `<Tag>` and namespace-prefixed `<ns:Tag>` forms.
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    // Build needles: bare `<Tag` and prefixed `:Tag`
    let bare = format!("<{}", tag);
    let prefixed = format!(":{}", tag);

    // Find opening tag position — try bare first, then any namespace prefix
    let start_tag_begin = xml.find(&bare).or_else(|| {
        // Find `:Tag` then walk back to the `<`
        xml.find(&prefixed)
            .and_then(|colon_pos| xml[..colon_pos].rfind('<'))
    })?;

    // Find end of opening tag `>`
    let tag_end = xml[start_tag_begin..].find('>')?;
    let content_start = start_tag_begin + tag_end + 1;

    // Find closing tag — try `</Tag>`, then `</ns:Tag>`
    let close_bare = format!("</{}>", tag);
    let close_prefixed = format!(":{}>", tag);

    let close_pos = if let Some(p) = xml[content_start..].find(&close_bare) {
        p
    } else {
        // Look for `:</Tag>` pattern with namespace prefix
        xml[content_start..].find(&close_prefixed).and_then(|cp| {
            // Walk back to find the `</`
            xml[content_start..content_start + cp].rfind("</")
        })?
    };

    Some(xml[content_start..content_start + close_pos].to_string())
}

/// Extract a URI from ONVIF SOAP response (looks for <tt:Uri> or similar).
fn extract_uri_from_soap(xml: &str) -> Option<String> {
    extract_xml_value(xml, "Uri")
}

/// Extract profile token from GetProfiles response.
fn extract_profile_token(xml: &str) -> Option<String> {
    // Look for token attribute in Profiles element
    if let Some(pos) = xml.find("token=\"") {
        let after = &xml[pos + 7..];
        if let Some(end) = after.find('"') {
            return Some(after[..end].to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// ONVIF IP camera adapter backed by real HTTP SOAP calls.
pub struct OnvifAdapter {
    cameras: HashMap<String, OnvifConfig>,
    client: reqwest::Client,
    base_url_override: Option<String>,
}

impl OnvifAdapter {
    pub fn new() -> Self {
        Self {
            cameras: HashMap::new(),
            client: reqwest::Client::new(),
            base_url_override: None,
        }
    }

    /// Override the base URL for all cameras (for testing with httpmock).
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url_override = Some(url.trim_end_matches('/').to_string());
        self
    }

    pub fn register_camera(&mut self, device_id: &str, config: OnvifConfig) {
        self.cameras.insert(device_id.to_string(), config);
    }

    fn get_config(&self, device_id: &str) -> anyhow::Result<&OnvifConfig> {
        self.cameras
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown camera: {}", device_id))
    }

    /// Build the ONVIF service URL for a camera.
    fn service_url(&self, config: &OnvifConfig, service_path: &str) -> String {
        match &self.base_url_override {
            Some(base) => format!("{}{}", base, service_path),
            None => format!("http://{}:{}{}", config.host, config.port, service_path),
        }
    }

    /// Build RTSP stream URL.
    fn rtsp_url(&self, config: &OnvifConfig) -> String {
        format!(
            "rtsp://{}:{}@{}:{}/stream1",
            config.username, config.password, config.host, config.rtsp_port
        )
    }

    /// Send a SOAP request and return the response body.
    async fn soap_request(
        &self,
        url: &str,
        soap_action: &str,
        body: &str,
    ) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .header("SOAPAction", soap_action)
            .body(body.to_string())
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("ONVIF SOAP request failed ({}): {}", status.as_u16(), text);
        }
        Ok(text)
    }

    /// Get the first media profile token.
    pub async fn get_profiles(&self, device_id: &str) -> anyhow::Result<String> {
        let config = self.get_config(device_id)?;
        let url = self.service_url(config, "/onvif/media_service");
        let body = soap_get_profiles();
        let resp = self.soap_request(&url, "GetProfiles", &body).await?;
        extract_profile_token(&resp)
            .ok_or_else(|| anyhow::anyhow!("no profile token in ONVIF response"))
    }

    /// Get the RTSP stream URI for a profile.
    pub async fn get_stream_uri(
        &self,
        device_id: &str,
        profile_token: &str,
    ) -> anyhow::Result<String> {
        let config = self.get_config(device_id)?;
        let url = self.service_url(config, "/onvif/media_service");
        let body = soap_get_stream_uri(profile_token);
        let resp = self.soap_request(&url, "GetStreamUri", &body).await?;
        extract_uri_from_soap(&resp)
            .ok_or_else(|| anyhow::anyhow!("no URI in GetStreamUri response"))
    }

    /// Get the snapshot URI for a profile.
    pub async fn get_snapshot_uri(
        &self,
        device_id: &str,
        profile_token: &str,
    ) -> anyhow::Result<String> {
        let config = self.get_config(device_id)?;
        let url = self.service_url(config, "/onvif/media_service");
        let body = soap_get_snapshot_uri(profile_token);
        let resp = self.soap_request(&url, "GetSnapshotUri", &body).await?;
        extract_uri_from_soap(&resp)
            .ok_or_else(|| anyhow::anyhow!("no URI in GetSnapshotUri response"))
    }

    /// PTZ continuous move.
    pub async fn continuous_move(
        &self,
        device_id: &str,
        profile_token: &str,
        pan: f64,
        tilt: f64,
        zoom: f64,
    ) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;
        let url = self.service_url(config, "/onvif/ptz_service");
        let body = soap_continuous_move(profile_token, pan, tilt, zoom);
        self.soap_request(&url, "ContinuousMove", &body).await?;
        Ok(())
    }

    /// PTZ stop.
    pub async fn ptz_stop(&self, device_id: &str, profile_token: &str) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;
        let url = self.service_url(config, "/onvif/ptz_service");
        let body = soap_stop(profile_token);
        self.soap_request(&url, "Stop", &body).await?;
        Ok(())
    }

    /// PTZ absolute move.
    pub async fn absolute_move(
        &self,
        device_id: &str,
        profile_token: &str,
        pan: f64,
        tilt: f64,
        zoom: f64,
    ) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;
        let url = self.service_url(config, "/onvif/ptz_service");
        let body = soap_absolute_move(profile_token, pan, tilt, zoom);
        self.soap_request(&url, "AbsoluteMove", &body).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DeviceController impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for OnvifAdapter {
    async fn turn_on(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("cameras are typically always-on")
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("cameras do not support turn_off via ONVIF")
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let config = self.get_config(device_id)?;
        match property {
            "ptz" => {
                let pan = value.get("pan").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let tilt = value.get("tilt").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let zoom = value.get("zoom").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let profile_token = value
                    .get("profile_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("profile_1");
                let url = self.service_url(config, "/onvif/ptz_service");
                let body = soap_continuous_move(profile_token, pan, tilt, zoom);
                self.soap_request(&url, "ContinuousMove", &body).await?;
                Ok(())
            }
            "ptz_stop" => {
                let profile_token = value.as_str().unwrap_or("profile_1");
                let url = self.service_url(config, "/onvif/ptz_service");
                let body = soap_stop(profile_token);
                self.soap_request(&url, "Stop", &body).await?;
                Ok(())
            }
            "snapshot" => {
                let profile_token = value.as_str().unwrap_or("profile_1");
                let url = self.service_url(config, "/onvif/media_service");
                let body = soap_get_snapshot_uri(profile_token);
                self.soap_request(&url, "GetSnapshotUri", &body).await?;
                Ok(())
            }
            _ => anyhow::bail!("unsupported camera property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let config = self.get_config(device_id)?;
        let url = self.service_url(config, "/onvif/media_service");
        let body = soap_get_profiles();
        let resp = self.soap_request(&url, "GetProfiles", &body).await?;
        let profile_token = extract_profile_token(&resp);
        let rtsp = self.rtsp_url(config);
        Ok(serde_json::json!({
            "host": config.host,
            "rtsp_url": rtsp,
            "status": "streaming",
            "profile_token": profile_token,
        }))
    }

    fn supported_type(&self) -> &str {
        "Camera"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn test_config() -> OnvifConfig {
        OnvifConfig {
            host: "192.168.1.60".into(),
            port: 80,
            username: "admin".into(),
            password: "pass123".into(),
            rtsp_port: 554,
        }
    }

    fn adapter(server: &MockServer) -> OnvifAdapter {
        let mut a = OnvifAdapter::new().with_base_url(&server.base_url());
        a.register_camera("cam-001", test_config());
        a
    }

    fn profiles_response() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
          <s:Body>
            <trt:GetProfilesResponse xmlns:trt="http://www.onvif.org/ver10/media/wsdl">
              <trt:Profiles token="profile_1" fixed="true">
                <tt:Name>MainStream</tt:Name>
              </trt:Profiles>
            </trt:GetProfilesResponse>
          </s:Body>
        </s:Envelope>"#
            .to_string()
    }

    fn stream_uri_response() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
          <s:Body>
            <trt:GetStreamUriResponse>
              <trt:MediaUri>
                <tt:Uri>rtsp://192.168.1.60:554/stream1</tt:Uri>
              </trt:MediaUri>
            </trt:GetStreamUriResponse>
          </s:Body>
        </s:Envelope>"#
            .to_string()
    }

    fn snapshot_uri_response() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
          <s:Body>
            <trt:GetSnapshotUriResponse>
              <trt:MediaUri>
                <tt:Uri>http://192.168.1.60/snapshot.jpg</tt:Uri>
              </trt:MediaUri>
            </trt:GetSnapshotUriResponse>
          </s:Body>
        </s:Envelope>"#
            .to_string()
    }

    fn ptz_ok_response() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
          <s:Body>
            <tptz:ContinuousMoveResponse/>
          </s:Body>
        </s:Envelope>"#
            .to_string()
    }

    // 1. supported_type
    #[test]
    fn supported_type_is_camera() {
        assert_eq!(OnvifAdapter::new().supported_type(), "Camera");
    }

    // 2. turn_on not supported
    #[tokio::test]
    async fn turn_on_not_supported() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a.turn_on("cam-001").await.is_err());
    }

    // 3. turn_off not supported
    #[tokio::test]
    async fn turn_off_not_supported() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a.turn_off("cam-001").await.is_err());
    }

    // 4. get_profiles returns token
    #[tokio::test]
    async fn get_profiles_returns_token() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/onvif/media_service");
            then.status(200).body(profiles_response());
        });
        let a = adapter(&server);
        let token = a.get_profiles("cam-001").await.unwrap();
        assert_eq!(token, "profile_1");
        mock.assert();
    }

    // 5. get_stream_uri returns URI
    #[tokio::test]
    async fn get_stream_uri_returns_uri() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/onvif/media_service");
            then.status(200).body(stream_uri_response());
        });
        let a = adapter(&server);
        let uri = a.get_stream_uri("cam-001", "profile_1").await.unwrap();
        assert!(uri.starts_with("rtsp://"));
        mock.assert();
    }

    // 6. get_snapshot_uri returns URI
    #[tokio::test]
    async fn get_snapshot_uri_returns_uri() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/onvif/media_service");
            then.status(200).body(snapshot_uri_response());
        });
        let a = adapter(&server);
        let uri = a.get_snapshot_uri("cam-001", "profile_1").await.unwrap();
        assert!(uri.contains("snapshot.jpg"));
        mock.assert();
    }

    // 7. continuous_move sends SOAP to PTZ service
    #[tokio::test]
    async fn continuous_move_sends_soap() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/onvif/ptz_service")
                .body_includes("ContinuousMove");
            then.status(200).body(ptz_ok_response());
        });
        let a = adapter(&server);
        a.continuous_move("cam-001", "profile_1", 0.5, -0.3, 0.0)
            .await
            .unwrap();
        mock.assert();
    }

    // 8. ptz_stop sends Stop SOAP
    #[tokio::test]
    async fn ptz_stop_sends_soap() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/onvif/ptz_service")
                .body_includes("Stop");
            then.status(200).body(ptz_ok_response());
        });
        let a = adapter(&server);
        a.ptz_stop("cam-001", "profile_1").await.unwrap();
        mock.assert();
    }

    // 9. absolute_move sends AbsoluteMove SOAP
    #[tokio::test]
    async fn absolute_move_sends_soap() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/onvif/ptz_service")
                .body_includes("AbsoluteMove");
            then.status(200).body(ptz_ok_response());
        });
        let a = adapter(&server);
        a.absolute_move("cam-001", "profile_1", 1.0, 0.0, 0.5)
            .await
            .unwrap();
        mock.assert();
    }

    // 10. set_property ptz sends ContinuousMove
    #[tokio::test]
    async fn set_property_ptz() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/onvif/ptz_service")
                .body_includes("ContinuousMove");
            then.status(200).body(ptz_ok_response());
        });
        let a = adapter(&server);
        a.set_property(
            "cam-001",
            "ptz",
            serde_json::json!({"pan": 0.5, "tilt": -0.3, "profile_token": "profile_1"}),
        )
        .await
        .unwrap();
        mock.assert();
    }

    // 11. set_property ptz_stop
    #[tokio::test]
    async fn set_property_ptz_stop() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/onvif/ptz_service")
                .body_includes("Stop");
            then.status(200).body(ptz_ok_response());
        });
        let a = adapter(&server);
        a.set_property("cam-001", "ptz_stop", serde_json::json!("profile_1"))
            .await
            .unwrap();
        mock.assert();
    }

    // 12. set_property snapshot sends GetSnapshotUri
    #[tokio::test]
    async fn set_property_snapshot() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/onvif/media_service")
                .body_includes("GetSnapshotUri");
            then.status(200).body(snapshot_uri_response());
        });
        let a = adapter(&server);
        a.set_property("cam-001", "snapshot", serde_json::json!("profile_1"))
            .await
            .unwrap();
        mock.assert();
    }

    // 13. set_property unsupported property errors
    #[tokio::test]
    async fn set_property_unsupported_errors() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a.set_property("cam-001", "fly", Value::Null).await.is_err());
    }

    // 14. get_state calls GetProfiles and returns state
    #[tokio::test]
    async fn get_state_returns_state() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/onvif/media_service");
            then.status(200).body(profiles_response());
        });
        let a = adapter(&server);
        let state = a.get_state("cam-001").await.unwrap();
        assert_eq!(state["status"], "streaming");
        assert!(state["rtsp_url"].as_str().unwrap().starts_with("rtsp://"));
        assert_eq!(state["profile_token"], "profile_1");
    }

    // 15. unknown camera errors
    #[tokio::test]
    async fn unknown_camera_errors() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a.get_state("unknown").await.is_err());
    }

    // 16. SOAP error status propagates
    #[tokio::test]
    async fn soap_error_propagates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/onvif/media_service");
            then.status(500).body("Internal Server Error");
        });
        let a = adapter(&server);
        assert!(a.get_profiles("cam-001").await.is_err());
    }

    // 17. extract_profile_token works
    #[test]
    fn extract_profile_token_parses() {
        let xml = r#"<Profiles token="main_profile" fixed="true">"#;
        assert_eq!(extract_profile_token(xml).unwrap(), "main_profile");
    }

    // 18. extract_uri_from_soap works
    #[test]
    fn extract_uri_parses() {
        let xml = r#"<tt:Uri>rtsp://example.com/stream</tt:Uri>"#;
        assert_eq!(
            extract_uri_from_soap(xml).unwrap(),
            "rtsp://example.com/stream"
        );
    }
}
