//! 浏览器动作定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 浏览器动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BrowserAction {
    Navigate { url: String },
    Click { selector: String },
    Type { selector: String, text: String },
    Screenshot { full_page: bool },
    ExtractText { selector: String },
    WaitFor { selector: String, timeout_ms: u64 },
    ScrollTo { selector: String },
    FillForm { fields: HashMap<String, String> },
}

/// 浏览器执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserResult {
    pub success: bool,
    pub data: Option<String>,
    pub screenshot: Option<Vec<u8>>,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

impl BrowserResult {
    pub fn ok(data: String) -> Self {
        Self {
            success: true,
            data: Some(data),
            screenshot: None,
            error: None,
            elapsed_ms: 0,
        }
    }

    pub fn err(msg: String) -> Self {
        Self {
            success: false,
            data: None,
            screenshot: None,
            error: Some(msg),
            elapsed_ms: 0,
        }
    }
}
