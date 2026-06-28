//! 适配器配置系统
//!
//! 统一管理所有外部服务适配器的连接配置：base_url、api_key、超时等。

use serde::{Deserialize, Serialize};

/// 邮件适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAdapterConfig {
    /// REST API 基础 URL（如 Gmail/Outlook）
    #[serde(default = "default_email_base_url")]
    pub base_url: String,
    /// OAuth2 access token
    #[serde(default)]
    pub access_token: String,
    /// 提供商名称 (gmail / outlook)
    #[serde(default = "default_email_provider")]
    pub provider: String,
    /// 请求超时（秒）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_email_base_url() -> String {
    "https://www.googleapis.com/gmail/v1".into()
}
fn default_email_provider() -> String {
    "gmail".into()
}

impl Default for EmailAdapterConfig {
    fn default() -> Self {
        Self {
            base_url: default_email_base_url(),
            access_token: String::new(),
            provider: default_email_provider(),
            timeout_secs: default_timeout(),
        }
    }
}

/// CalDAV 日历适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalDavConfig {
    #[serde(default = "default_caldav_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_caldav_base_url() -> String {
    "https://caldav.example.com".into()
}

impl Default for CalDavConfig {
    fn default() -> Self {
        Self {
            base_url: default_caldav_base_url(),
            username: String::new(),
            password: String::new(),
            timeout_secs: default_timeout(),
        }
    }
}

/// 搜索 API 适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchApiConfig {
    #[serde(default = "default_search_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub engine_id: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_search_base_url() -> String {
    "https://www.googleapis.com/customsearch/v1".into()
}

impl Default for SearchApiConfig {
    fn default() -> Self {
        Self {
            base_url: default_search_base_url(),
            api_key: String::new(),
            engine_id: String::new(),
            timeout_secs: default_timeout(),
        }
    }
}

/// 媒体控制适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAdapterConfig {
    #[serde(default = "default_media_base_url")]
    pub base_url: String,
    #[serde(default = "default_media_provider")]
    pub provider_name: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_media_base_url() -> String {
    "http://localhost:5005".into()
}
fn default_media_provider() -> String {
    "sonos".into()
}

impl Default for MediaAdapterConfig {
    fn default() -> Self {
        Self {
            base_url: default_media_base_url(),
            provider_name: default_media_provider(),
            timeout_secs: default_timeout(),
        }
    }
}

/// 语音适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceAdapterConfig {
    /// STT API URL
    #[serde(default = "default_stt_url")]
    pub stt_url: String,
    /// TTS API URL
    #[serde(default = "default_tts_url")]
    pub tts_url: String,
    #[serde(default)]
    pub api_key: String,
    /// 模型名称
    #[serde(default = "default_voice_model")]
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_stt_url() -> String {
    "https://api.openai.com/v1/audio/transcriptions".into()
}
fn default_tts_url() -> String {
    "https://api.openai.com/v1/audio/speech".into()
}
fn default_voice_model() -> String {
    "whisper-1".into()
}

impl Default for VoiceAdapterConfig {
    fn default() -> Self {
        Self {
            stt_url: default_stt_url(),
            tts_url: default_tts_url(),
            api_key: String::new(),
            model: default_voice_model(),
            timeout_secs: default_timeout(),
        }
    }
}

/// 推理后端适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceAdapterConfig {
    #[serde(default = "default_inference_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_inference_model")]
    pub model_name: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_inference_base_url() -> String {
    "http://localhost:11434".into()
}
fn default_inference_model() -> String {
    "llama3".into()
}

impl Default for InferenceAdapterConfig {
    fn default() -> Self {
        Self {
            base_url: default_inference_base_url(),
            api_key: String::new(),
            model_name: default_inference_model(),
            timeout_secs: default_timeout(),
        }
    }
}

/// 浏览器自动化适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserAdapterConfig {
    #[serde(default = "default_browser_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_browser_base_url() -> String {
    "http://localhost:9222".into()
}

impl Default for BrowserAdapterConfig {
    fn default() -> Self {
        Self {
            base_url: default_browser_base_url(),
            api_key: String::new(),
            timeout_secs: default_timeout(),
        }
    }
}

/// 屏幕分析适配器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenAdapterConfig {
    /// OCR API URL
    #[serde(default = "default_ocr_url")]
    pub ocr_url: String,
    /// 屏幕分析 API URL
    #[serde(default = "default_analyzer_url")]
    pub analyzer_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_ocr_url() -> String {
    "http://localhost:8080/ocr".into()
}
fn default_analyzer_url() -> String {
    "http://localhost:8080/analyze".into()
}

impl Default for ScreenAdapterConfig {
    fn default() -> Self {
        Self {
            ocr_url: default_ocr_url(),
            analyzer_url: default_analyzer_url(),
            api_key: String::new(),
            timeout_secs: default_timeout(),
        }
    }
}

fn default_timeout() -> u64 {
    30
}

/// 统一适配器配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterConfig {
    #[serde(default)]
    pub email: Option<EmailAdapterConfig>,
    #[serde(default)]
    pub calendar: Option<CalDavConfig>,
    #[serde(default)]
    pub search: Option<SearchApiConfig>,
    #[serde(default)]
    pub media: Option<MediaAdapterConfig>,
    #[serde(default)]
    pub voice: Option<VoiceAdapterConfig>,
    #[serde(default)]
    pub inference: Option<InferenceAdapterConfig>,
    #[serde(default)]
    pub browser: Option<BrowserAdapterConfig>,
    #[serde(default)]
    pub screen: Option<ScreenAdapterConfig>,
    #[serde(default)]
    pub homeassistant: Option<crate::iot::adapters::homeassistant::HomeAssistantConfig>,
    #[serde(default)]
    pub tuya: Option<crate::iot::adapters::tuya::TuyaConfig>,
    #[serde(default)]
    pub mqtt: Option<crate::iot::adapters::mqtt::MqttConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_config_default_is_empty() {
        let cfg = AdapterConfig::default();
        assert!(cfg.email.is_none());
        assert!(cfg.calendar.is_none());
        assert!(cfg.search.is_none());
        assert!(cfg.media.is_none());
        assert!(cfg.voice.is_none());
        assert!(cfg.inference.is_none());
        assert!(cfg.browser.is_none());
        assert!(cfg.screen.is_none());
        assert!(cfg.homeassistant.is_none());
        assert!(cfg.tuya.is_none());
        assert!(cfg.mqtt.is_none());
    }

    #[test]
    fn email_config_defaults() {
        let cfg = EmailAdapterConfig::default();
        assert!(cfg.base_url.contains("googleapis.com"));
        assert_eq!(cfg.provider, "gmail");
        assert_eq!(cfg.timeout_secs, 30);
    }

    #[test]
    fn caldav_config_defaults() {
        let cfg = CalDavConfig::default();
        assert!(cfg.base_url.contains("caldav"));
        assert!(cfg.username.is_empty());
    }

    #[test]
    fn voice_config_defaults() {
        let cfg = VoiceAdapterConfig::default();
        assert!(cfg.stt_url.contains("transcriptions"));
        assert!(cfg.tts_url.contains("speech"));
        assert_eq!(cfg.model, "whisper-1");
    }

    #[test]
    fn inference_config_defaults() {
        let cfg = InferenceAdapterConfig::default();
        assert!(cfg.base_url.contains("11434"));
        assert_eq!(cfg.model_name, "llama3");
    }

    #[test]
    fn adapter_config_serde_roundtrip() {
        let mut cfg = AdapterConfig::default();
        cfg.email = Some(EmailAdapterConfig::default());
        cfg.voice = Some(VoiceAdapterConfig::default());
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AdapterConfig = serde_json::from_str(&json).unwrap();
        assert!(back.email.is_some());
        assert!(back.voice.is_some());
        assert!(back.calendar.is_none());
    }

    #[test]
    fn adapter_config_from_partial_json() {
        let json = r#"{"email": {"access_token": "tok123"}}"#;
        let cfg: AdapterConfig = serde_json::from_str(json).unwrap();
        let email = cfg.email.unwrap();
        assert_eq!(email.access_token, "tok123");
        assert!(email.base_url.contains("googleapis.com"));
    }

    #[test]
    fn screen_config_defaults() {
        let cfg = ScreenAdapterConfig::default();
        assert!(cfg.ocr_url.contains("ocr"));
        assert!(cfg.analyzer_url.contains("analyze"));
    }

    #[test]
    fn iot_homeassistant_from_json() {
        let json = r#"{"homeassistant": {"host": "192.168.1.10", "port": 8123, "access_token": "tok"}}"#;
        let cfg: AdapterConfig = serde_json::from_str(json).unwrap();
        let ha = cfg.homeassistant.unwrap();
        assert_eq!(ha.host, "192.168.1.10");
        assert_eq!(ha.port, 8123);
        assert_eq!(ha.access_token, "tok");
    }

    #[test]
    fn iot_mqtt_serde_roundtrip() {
        let mut cfg = AdapterConfig::default();
        cfg.mqtt = Some(crate::iot::adapters::mqtt::MqttConfig::default());
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AdapterConfig = serde_json::from_str(&json).unwrap();
        assert!(back.mqtt.is_some());
        assert!(back.homeassistant.is_none());
        assert!(back.tuya.is_none());
    }

    #[test]
    fn iot_tuya_config_serde_roundtrip() {
        use crate::iot::adapters::tuya::{TuyaConfig, TuyaRegion};
        let cfg = TuyaConfig {
            client_id: "cid".into(),
            client_secret: "secret".into(),
            region: TuyaRegion::EU,
            uid: "uid123".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: TuyaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.client_id, "cid");
        assert_eq!(back.region, TuyaRegion::EU);
    }

    #[test]
    fn iot_tuya_region_serde() {
        use crate::iot::adapters::tuya::TuyaRegion;
        let json = serde_json::to_string(&TuyaRegion::CN).unwrap();
        let back: TuyaRegion = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TuyaRegion::CN);
    }
}
