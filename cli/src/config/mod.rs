//! CLI 配置管理模块
//!
//! 支持 TOML 配置文件，提供默认值和环境变量覆盖。

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// CLI 全局配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// 默认模型
    #[serde(default = "default_model")]
    pub default_model: String,

    /// 路由策略: balanced | quality | cost | speed
    #[serde(default = "default_strategy")]
    pub routing_strategy: String,

    /// 每月预算上限 (USD)
    #[serde(default = "default_budget")]
    pub monthly_budget: f64,

    /// 每次请求最大 token 数
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// 温度参数
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// 是否显示路由信息
    #[serde(default = "default_show_route")]
    pub show_route_info: bool,

    /// 是否启用彩色输出
    #[serde(default = "default_true")]
    pub color_output: bool,

    /// 历史记录最大条数
    #[serde(default = "default_history_size")]
    pub history_size: usize,

    /// 数据目录
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}
fn default_model() -> String {
    "qwen2.5-3b-local".to_string()
}
fn default_strategy() -> String {
    "balanced".to_string()
}
fn default_budget() -> f64 {
    10.0
}
fn default_max_tokens() -> usize {
    2048
}
fn default_temperature() -> f32 {
    0.7
}
fn default_show_route() -> bool {
    true
}
fn default_true() -> bool {
    true
}
fn default_history_size() -> usize {
    1000
}
fn default_data_dir() -> PathBuf {
    data_dir()
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            routing_strategy: default_strategy(),
            monthly_budget: default_budget(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            show_route_info: default_show_route(),
            color_output: default_true(),
            history_size: default_history_size(),
            data_dir: default_data_dir(),
        }
    }
}
impl CliConfig {
    /// 加载配置文件，不存在则创建默认配置
    pub fn load() -> Result<Self> {
        let config_path = config_file_path();
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: CliConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<()> {
        let config_path = config_file_path();
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// 设置单个配置项
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "default_model" => self.default_model = value.to_string(),
            "routing_strategy" => self.routing_strategy = value.to_string(),
            "monthly_budget" => self.monthly_budget = value.parse()?,
            "max_tokens" => self.max_tokens = value.parse()?,
            "temperature" => self.temperature = value.parse()?,
            "show_route_info" => self.show_route_info = value.parse()?,
            "color_output" => self.color_output = value.parse()?,
            "history_size" => self.history_size = value.parse()?,
            _ => anyhow::bail!("未知配置项: {key}"),
        }
        self.save()?;
        Ok(())
    }

    /// 获取单个配置项的字符串表示
    pub fn get(&self, key: &str) -> Result<String> {
        match key {
            "default_model" => Ok(self.default_model.clone()),
            "routing_strategy" => Ok(self.routing_strategy.clone()),
            "monthly_budget" => Ok(self.monthly_budget.to_string()),
            "max_tokens" => Ok(self.max_tokens.to_string()),
            "temperature" => Ok(self.temperature.to_string()),
            "show_route_info" => Ok(self.show_route_info.to_string()),
            "color_output" => Ok(self.color_output.to_string()),
            "history_size" => Ok(self.history_size.to_string()),
            "data_dir" => Ok(self.data_dir.to_string_lossy().to_string()),
            _ => anyhow::bail!("未知配置项: {key}"),
        }
    }

    /// 列出所有配置项
    pub fn list_keys() -> &'static [&'static str] {
        &[
            "default_model",
            "routing_strategy",
            "monthly_budget",
            "max_tokens",
            "temperature",
            "show_route_info",
            "color_output",
            "history_size",
            "data_dir",
        ]
    }
}

/// 获取数据目录
pub fn data_dir() -> PathBuf {
    if let Some(dir) = dirs::data_dir() {
        return dir.join("prometheus-ai");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".prometheus");
    }
    PathBuf::from("/tmp/prometheus-ai")
}

/// 获取配置文件路径
pub fn config_file_path() -> PathBuf {
    if let Some(dir) = dirs::config_dir() {
        return dir.join("prometheus-ai").join("config.toml");
    }
    data_dir().join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = CliConfig::default();
        assert_eq!(config.default_model, "qwen2.5-3b-local");
        assert_eq!(config.max_tokens, 2048);
        assert!(config.color_output);
    }

    #[test]
    fn set_and_get_config_value() {
        let mut config = CliConfig::default();
        config.set("max_tokens", "4096").unwrap();
        assert_eq!(config.get("max_tokens").unwrap(), "4096");
    }

    #[test]
    fn unknown_key_returns_error() {
        let config = CliConfig::default();
        assert!(config.get("nonexistent").is_err());
    }
}
