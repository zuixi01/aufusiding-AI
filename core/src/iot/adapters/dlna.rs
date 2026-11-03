//! DLNA/UPnP media renderer adapter
//!
//! Controls DLNA speakers, smart TVs, and media renderers via UPnP AVTransport SOAP.
//! Capabilities: MediaPlayback, VolumeControl, ScreenMirror

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::iot::control::DeviceController;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// DLNA device info.
#[derive(Debug, Clone)]
pub struct DlnaDevice {
    pub control_url: String,
    pub friendly_name: String,
}

// ---------------------------------------------------------------------------
// SOAP helpers
// ---------------------------------------------------------------------------

const AVTRANSPORT_URN: &str = "urn:schemas-upnp-org:service:AVTransport:1";
const RENDERING_CONTROL_URN: &str = "urn:schemas-upnp-org:service:RenderingControl:1";

fn soap_envelope(service_urn: &str, action: &str, body_xml: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action} xmlns:u="{urn}">
      {body}
    </u:{action}>
  </s:Body>
</s:Envelope>"#,
        action = action,
        urn = service_urn,
        body = body_xml,
    )
}

fn soap_set_av_transport_uri(uri: &str) -> String {
    soap_envelope(
        AVTRANSPORT_URN,
        "SetAVTransportURI",
        &format!(
            "<InstanceID>0</InstanceID>\
             <CurrentURI>{}</CurrentURI>\
             <CurrentURIMetaData></CurrentURIMetaData>",
            uri
        ),
    )
}

fn soap_play() -> String {
    soap_envelope(
        AVTRANSPORT_URN,
        "Play",
        "<InstanceID>0</InstanceID><Speed>1</Speed>",
    )
}

fn soap_pause() -> String {
    soap_envelope(AVTRANSPORT_URN, "Pause", "<InstanceID>0</InstanceID>")
}

fn soap_stop() -> String {
    soap_envelope(AVTRANSPORT_URN, "Stop", "<InstanceID>0</InstanceID>")
}

fn soap_set_volume(volume: u64) -> String {
    soap_envelope(
        RENDERING_CONTROL_URN,
        "SetVolume",
        &format!(
            "<InstanceID>0</InstanceID>\
             <Channel>Master</Channel>\
             <DesiredVolume>{}</DesiredVolume>",
            volume
        ),
    )
}

fn soap_get_transport_info() -> String {
    soap_envelope(
        AVTRANSPORT_URN,
        "GetTransportInfo",
        "<InstanceID>0</InstanceID>",
    )
}

/// Extract text between simple XML tags.
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = xml.find(&open) {
        let content_start = start + open.len();
        if let Some(end) = xml[content_start..].find(&close) {
            return Some(xml[content_start..content_start + end].to_string());
        }
    }
    // Try with namespace prefix
    let colon_tag = format!(":{}>", tag);
    for (i, _) in xml.match_indices(&colon_tag) {
        // Walk backwards to find '<'
        let slice = &xml[..i + colon_tag.len()];
        if let Some(lt) = slice.rfind('<') {
            let content_start = i + colon_tag.len();
            let close_colon = format!(":{}>", tag);
            if let Some(close_pos) = xml[content_start..].find("</") {
                let after = &xml[content_start + close_pos..];
                if after.contains(&close_colon) {
                    let _ = lt; // used above
                    return Some(xml[content_start..content_start + close_pos].to_string());
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// DLNA/UPnP media renderer adapter backed by real HTTP SOAP calls.
pub struct DlnaAdapter {
    renderers: HashMap<String, DlnaDevice>,
    client: reqwest::Client,
    base_url_override: Option<String>,
}

impl DlnaAdapter {
    pub fn new() -> Self {
        Self {
            renderers: HashMap::new(),
            client: reqwest::Client::new(),
            base_url_override: None,
        }
    }

    /// Override the base URL for all devices (for testing with httpmock).
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url_override = Some(url.trim_end_matches('/').to_string());
        self
    }

    pub fn register_renderer(&mut self, device_id: &str, device: DlnaDevice) {
        self.renderers.insert(device_id.to_string(), device);
    }

    fn get_device(&self, device_id: &str) -> anyhow::Result<&DlnaDevice> {
        self.renderers
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown DLNA device: {}", device_id))
    }

    /// Resolve the control URL, using override if set.
    fn control_url(&self, device: &DlnaDevice, path: &str) -> String {
        match &self.base_url_override {
            Some(base) => format!("{}{}", base, path),
            None => device.control_url.clone(),
        }
    }

    /// Send a UPnP SOAP action.
    async fn soap_action(
        &self,
        url: &str,
        service_urn: &str,
        action: &str,
        body: &str,
    ) -> anyhow::Result<String> {
        let soap_action_header = format!("\"{}#{}\"", service_urn, action);
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "text/xml; charset=utf-8")
            .header("SOAPAction", soap_action_header)
            .body(body.to_string())
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!(
                "DLNA SOAP {} failed ({}): {}",
                action,
                status.as_u16(),
                text
            );
        }
        Ok(text)
    }

    /// Set the media URI and start playback.
    pub async fn play_media(&self, device_id: &str, media_url: &str) -> anyhow::Result<()> {
        let device = self.get_device(device_id)?;
        let url = self.control_url(device, "/AVTransport/Control");
        let set_body = soap_set_av_transport_uri(media_url);
        self.soap_action(&url, AVTRANSPORT_URN, "SetAVTransportURI", &set_body)
            .await?;
        let play_body = soap_play();
        self.soap_action(&url, AVTRANSPORT_URN, "Play", &play_body)
            .await?;
        Ok(())
    }

    /// Pause playback.
    pub async fn pause(&self, device_id: &str) -> anyhow::Result<()> {
        let device = self.get_device(device_id)?;
        let url = self.control_url(device, "/AVTransport/Control");
        let body = soap_pause();
        self.soap_action(&url, AVTRANSPORT_URN, "Pause", &body)
            .await?;
        Ok(())
    }

    /// Stop playback.
    pub async fn stop(&self, device_id: &str) -> anyhow::Result<()> {
        let device = self.get_device(device_id)?;
        let url = self.control_url(device, "/AVTransport/Control");
        let body = soap_stop();
        self.soap_action(&url, AVTRANSPORT_URN, "Stop", &body)
            .await?;
        Ok(())
    }

    /// Set volume (0-100).
    pub async fn set_volume(&self, device_id: &str, volume: u64) -> anyhow::Result<()> {
        if volume > 100 {
            anyhow::bail!("volume must be 0-100, got {}", volume);
        }
        let device = self.get_device(device_id)?;
        let url = self.control_url(device, "/RenderingControl/Control");
        let body = soap_set_volume(volume);
        self.soap_action(&url, RENDERING_CONTROL_URN, "SetVolume", &body)
            .await?;
        Ok(())
    }

    /// Get transport info (playing/paused/stopped).
    pub async fn get_transport_info(&self, device_id: &str) -> anyhow::Result<String> {
        let device = self.get_device(device_id)?;
        let url = self.control_url(device, "/AVTransport/Control");
        let body = soap_get_transport_info();
        let resp = self
            .soap_action(&url, AVTRANSPORT_URN, "GetTransportInfo", &body)
            .await?;
        Ok(extract_xml_value(&resp, "CurrentTransportState")
            .unwrap_or_else(|| "UNKNOWN".to_string()))
    }
}

// ---------------------------------------------------------------------------
// DeviceController impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DeviceController for DlnaAdapter {
    async fn turn_on(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("DLNA devices are typically always-on")
    }

    async fn turn_off(&self, _device_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("DLNA devices do not support turn_off")
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        match property {
            "play" => {
                let url = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("play value must be a media URL string"))?;
                self.play_media(device_id, url).await
            }
            "pause" => self.pause(device_id).await,
            "stop" => self.stop(device_id).await,
            "volume" => {
                let vol = value
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("volume must be a number"))?;
                self.set_volume(device_id, vol).await
            }
            _ => anyhow::bail!("unsupported DLNA property: {}", property),
        }
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let device = self.get_device(device_id)?;
        let transport_state = self
            .get_transport_info(device_id)
            .await
            .unwrap_or_else(|_| "UNKNOWN".to_string());
        Ok(serde_json::json!({
            "name": device.friendly_name,
            "transport_state": transport_state,
        }))
    }

    fn supported_type(&self) -> &str {
        "Speaker"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn adapter(server: &MockServer) -> DlnaAdapter {
        let mut a = DlnaAdapter::new().with_base_url(&server.base_url());
        a.register_renderer(
            "speaker-001",
            DlnaDevice {
                control_url: "http://192.168.1.30:49152/AVTransport/Control".into(),
                friendly_name: "Living Room Speaker".into(),
            },
        );
        a
    }

    fn ok_response() -> String {
        r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:Response/></s:Body></s:Envelope>"#.to_string()
    }

    fn transport_info_response(state: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <u:GetTransportInfoResponse>
      <CurrentTransportState>{}</CurrentTransportState>
      <CurrentTransportStatus>OK</CurrentTransportStatus>
    </u:GetTransportInfoResponse>
  </s:Body>
</s:Envelope>"#,
            state
        )
    }

    // 1. supported_type
    #[test]
    fn supported_type_is_speaker() {
        assert_eq!(DlnaAdapter::new().supported_type(), "Speaker");
    }

    // 2. turn_on not supported
    #[tokio::test]
    async fn turn_on_not_supported() {
        let server = MockServer::start();
        assert!(adapter(&server).turn_on("speaker-001").await.is_err());
    }

    // 3. turn_off not supported
    #[tokio::test]
    async fn turn_off_not_supported() {
        let server = MockServer::start();
        assert!(adapter(&server).turn_off("speaker-001").await.is_err());
    }

    // 4. play_media sends SetAVTransportURI then Play
    #[tokio::test]
    async fn play_media_sends_two_soap_calls() {
        let server = MockServer::start();
        let set_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/AVTransport/Control")
                .body_includes("SetAVTransportURI")
                .body_includes("http://example.com/song.mp3");
            then.status(200).body(ok_response());
        });
        let play_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/AVTransport/Control")
                .body_includes("Play");
            then.status(200).body(ok_response());
        });
        let a = adapter(&server);
        a.play_media("speaker-001", "http://example.com/song.mp3")
            .await
            .unwrap();
        set_mock.assert();
        play_mock.assert();
    }

    // 5. pause sends Pause SOAP
    #[tokio::test]
    async fn pause_sends_soap() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/AVTransport/Control")
                .body_includes("Pause");
            then.status(200).body(ok_response());
        });
        let a = adapter(&server);
        a.pause("speaker-001").await.unwrap();
        mock.assert();
    }

    // 6. stop sends Stop SOAP
    #[tokio::test]
    async fn stop_sends_soap() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/AVTransport/Control")
                .body_includes("Stop");
            then.status(200).body(ok_response());
        });
        let a = adapter(&server);
        a.stop("speaker-001").await.unwrap();
        mock.assert();
    }

    // 7. set_volume sends SetVolume to RenderingControl
    #[tokio::test]
    async fn set_volume_sends_soap() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/RenderingControl/Control")
                .body_includes("SetVolume")
                .body_includes("<DesiredVolume>75</DesiredVolume>");
            then.status(200).body(ok_response());
        });
        let a = adapter(&server);
        a.set_volume("speaker-001", 75).await.unwrap();
        mock.assert();
    }

    // 8. set_volume rejects > 100
    #[tokio::test]
    async fn set_volume_rejects_over_100() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a.set_volume("speaker-001", 150).await.is_err());
    }

    // 9. get_transport_info parses state
    #[tokio::test]
    async fn get_transport_info_parses() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/AVTransport/Control")
                .body_includes("GetTransportInfo");
            then.status(200).body(transport_info_response("PLAYING"));
        });
        let a = adapter(&server);
        let state = a.get_transport_info("speaker-001").await.unwrap();
        assert_eq!(state, "PLAYING");
    }

    // 10. set_property play triggers play_media
    #[tokio::test]
    async fn set_property_play() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/AVTransport/Control");
            then.status(200).body(ok_response());
        });
        let a = adapter(&server);
        a.set_property(
            "speaker-001",
            "play",
            serde_json::json!("http://example.com/song.mp3"),
        )
        .await
        .unwrap();
    }

    // 11. set_property volume
    #[tokio::test]
    async fn set_property_volume() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/RenderingControl/Control");
            then.status(200).body(ok_response());
        });
        let a = adapter(&server);
        a.set_property("speaker-001", "volume", serde_json::json!(50))
            .await
            .unwrap();
    }

    // 12. set_property unsupported errors
    #[tokio::test]
    async fn set_property_unsupported_errors() {
        let server = MockServer::start();
        let a = adapter(&server);
        assert!(a
            .set_property("speaker-001", "fly", Value::Null)
            .await
            .is_err());
    }

    // 13. get_state returns transport state
    #[tokio::test]
    async fn get_state_returns_info() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/AVTransport/Control");
            then.status(200).body(transport_info_response("STOPPED"));
        });
        let a = adapter(&server);
        let state = a.get_state("speaker-001").await.unwrap();
        assert_eq!(state["name"], "Living Room Speaker");
        assert_eq!(state["transport_state"], "STOPPED");
    }

    // 14. unknown device errors
    #[tokio::test]
    async fn unknown_device_errors() {
        let server = MockServer::start();
        assert!(adapter(&server).get_state("unknown").await.is_err());
    }

    // 15. SOAP error propagates
    #[tokio::test]
    async fn soap_error_propagates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/AVTransport/Control");
            then.status(500).body("Server Error");
        });
        let a = adapter(&server);
        assert!(a.stop("speaker-001").await.is_err());
    }
}
