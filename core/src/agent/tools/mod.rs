//! Agent 工具系统 — 工具 trait、注册表、具体工具封装

pub mod registry;
pub mod data_analysis;
pub mod document;
pub mod spreadsheet;
pub mod web_search;
pub mod search_api;

use std::sync::Arc;

pub use registry::{AgentTool, ToolDescriptor, ToolRegistry, ToolResult};
pub use data_analysis::DataAnalysisTool;
pub use document::DocumentTool;
pub use spreadsheet::SpreadsheetTool;
pub use web_search::WebSearchTool;

/// 创建包含全部内置工具的默认工具集
pub fn default_tools() -> Vec<Arc<dyn AgentTool>> {
    vec![
        Arc::new(DataAnalysisTool::new()),
        Arc::new(DocumentTool::new()),
        Arc::new(SpreadsheetTool::new()),
        Arc::new(WebSearchTool::with_mock()),
    ]
}
