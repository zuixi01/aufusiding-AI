//! 全局配置模块
//!
//! 管理 Prometheus 的所有运行时配置，包括模型路径、API 密钥、存储位置等。

pub mod adapter_config;

pub use adapter_config::AdapterConfig;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 全局配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 数据存储根目录
    pub data_dir: PathBuf,
    /// 模型文件目录
    pub models_dir: PathBuf,
    /// 是否启用本地推理
    pub local_inference_enabled: bool,
    /// 默认云端 Provider
    pub default_provider: String,
    /// 日志级别
    pub log_level: String,
    /// 主题 ("light"/"dark"/"system")
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 语言 ("zh-CN"/"en-US")
    #[serde(default = "default_language")]
    pub language: String,
    /// 插件目录
    #[serde(default = "default_plugin_dir")]
    pub plugin_dir: PathBuf,
    /// 隐私同意数据库路径
    #[serde(default = "default_privacy_consent_db")]
    pub privacy_consent_db: PathBuf,
    /// 环境计算配置
    #[serde(default)]
    pub ambient: crate::ambient::AmbientConfig,
    /// 适配器配置
    #[serde(default)]
    pub adapter: AdapterConfig,
    /// 通用键值设置
    #[serde(default)]
    pub extra: HashMap<String, String>,
}

fn default_theme() -> String {
    "system".into()
}
fn default_language() -> String {
    "zh-CN".into()
}
fn default_plugin_dir() -> PathBuf {
    PathBuf::from("~/.prometheus/plugins")
}
fn default_privacy_consent_db() -> PathBuf {
    PathBuf::from("~/.prometheus/data/consent.db")
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("~/.prometheus/data"),
            models_dir: PathBuf::from("~/.prometheus/models"),
            local_inference_enabled: true,
            default_provider: "openai".to_string(),
            log_level: "info".to_string(),
            theme: default_theme(),
            language: default_language(),
            plugin_dir: default_plugin_dir(),
            privacy_consent_db: default_privacy_consent_db(),
            ambient: crate::ambient::AmbientConfig::default(),
            adapter: AdapterConfig::default(),
            extra: HashMap::new(),
        }
    }
}

impl AppConfig {
    /// 从配置文件加载
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        let mut config: Self = serde_json::from_str(&raw)?;
        config.expand_home_dir();
        Ok(config)
    }

    /// 保存到配置文件
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_string_pretty(self)?;
        std::fs::write(path, payload)?;
        Ok(())
    }

    /// 获取通用设置
    pub fn get_setting(&self, key: &str) -> Option<String> {
        match key {
            "theme" => Some(self.theme.clone()),
            "language" => Some(self.language.clone()),
            "log_level" => Some(self.log_level.clone()),
            "default_provider" => Some(self.default_provider.clone()),
            _ => self.extra.get(key).cloned(),
        }
    }

    /// 设置通用设置
    pub fn set_setting(&mut self, key: &str, value: String) {
        match key {
            "theme" => self.theme = value,
            "language" => self.language = value,
            "log_level" => self.log_level = value,
            "default_provider" => self.default_provider = value,
            _ => {
                self.extra.insert(key.to_string(), value);
            }
        }
    }

    fn expand_home_dir(&mut self) {
        self.data_dir = expand_tilde(&self.data_dir);
        self.models_dir = expand_tilde(&self.models_dir);
        self.plugin_dir = expand_tilde(&self.plugin_dir);
        self.privacy_consent_db = expand_tilde(&self.privacy_consent_db);
    }
}

fn expand_tilde(path: &std::path::Path) -> std::path::PathBuf {
    let raw = path.to_string_lossy();
    if !raw.starts_with("~/") {
        return path.to_path_buf();
    }
    match std::env::var("HOME") {
        Ok(home) => std::path::PathBuf::from(home).join(raw.trim_start_matches("~/")),
        Err(_) => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_new_fields() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.theme, "system");
        assert_eq!(cfg.language, "zh-CN");
        assert!(!cfg.plugin_dir.as_os_str().is_empty());
        assert!(!cfg.privacy_consent_db.as_os_str().is_empty());
    }

    #[test]
    fn default_config_has_adapter() {
        let cfg = AppConfig::default();
        assert!(cfg.adapter.email.is_none());
        assert!(cfg.adapter.voice.is_none());
    }

    #[test]
    fn get_set_known_settings() {
        let mut cfg = AppConfig::default();
        assert_eq!(cfg.get_setting("theme"), Some("system".into()));
        cfg.set_setting("theme", "dark".into());
        assert_eq!(cfg.get_setting("theme"), Some("dark".into()));
        assert_eq!(cfg.theme, "dark");
    }

    #[test]
    fn get_set_extra_settings() {
        let mut cfg = AppConfig::default();
        assert_eq!(cfg.get_setting("custom_key"), None);
        cfg.set_setting("custom_key", "custom_value".into());
        assert_eq!(cfg.get_setting("custom_key"), Some("custom_value".into()));
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let mut cfg = AppConfig::default();
        cfg.set_setting("theme", "dark".into());
        cfg.set_setting("custom", "val".into());
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.theme, "dark");
        assert_eq!(parsed.get_setting("custom"), Some("val".into()));
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let path = crate::test_support::temp_json_path("nonexistent-cfg");
        let cfg = AppConfig::load(path.as_path()).unwrap();
        assert_eq!(cfg.theme, "system");
    }

    #[test]
    fn save_and_load() {
        let path = crate::test_support::temp_json_path("config-test");
        let mut cfg = AppConfig::default();
        cfg.set_setting("language", "en-US".into());
        cfg.save(&path).unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        assert_eq!(loaded.language, "en-US");
    }
}
