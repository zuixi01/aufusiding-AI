//! Prometheus 插件开发 SDK
//!
//! 提供插件开发所需的 trait 和工具函数。

use serde::{Deserialize, Serialize};

/// 插件 trait
pub trait Plugin {
    /// 插件名称
    fn name(&self) -> &str;

    /// 插件版本
    fn version(&self) -> &str;

    /// 初始化插件
    fn init(&mut self) -> Result<(), String>;

    /// 执行插件功能
    fn execute(&self, input: &str) -> Result<String, String>;
}

/// 插件请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRequest {
    pub method: String,
    pub params: serde_json::Value,
}

/// 插件响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResponse {
    pub success: bool,
    pub data: serde_json::Value,
    pub error: Option<String>,
}
