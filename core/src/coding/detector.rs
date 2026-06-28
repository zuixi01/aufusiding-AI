//! 外部工具检测器
//!
//! 检测系统中已安装的编程工具 (Claude Code, Aider, Cursor 等)。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 检测到的外部工具
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedTool {
    pub name: String,
    pub binary: String,
    pub path: Option<String>,
}

/// 外部工具检测器
pub struct ExternalToolDetector {
    known_tools: Vec<(&'static str, &'static str)>,
}

impl ExternalToolDetector {
    pub fn new() -> Self {
        Self {
            known_tools: vec![
                ("claude-code", "claude"),
                ("aider", "aider"),
                ("cursor", "cursor"),
                ("github-copilot", "gh"),
                ("codex", "codex"),
            ],
        }
    }

    fn detect_tool(name: &str, binary: &str) -> Option<DetectedTool> {
        let output = std::process::Command::new("which")
            .arg(binary)
            .output()
            .ok()?;

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Some(DetectedTool {
                name: name.to_string(),
                binary: binary.to_string(),
                path: Some(path),
            })
        } else {
            None
        }
    }
}

#[async_trait]
impl AgentTool for ExternalToolDetector {
    fn name(&self) -> &str {
        "tool_detector"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "tool_detector".to_string(),
            description: "检测系统中已安装的编程工具".to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {},
            }),
        }
    }

    async fn execute(&self, _params: Value) -> ToolResult {
        let mut detected = Vec::new();
        for (name, binary) in &self.known_tools {
            if let Some(tool) = Self::detect_tool(name, binary) {
                detected.push(tool);
            }
        }
        let count = detected.len();
        ToolResult::ok(json!({ "tools": detected, "count": count }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detector_returns_results() {
        let detector = ExternalToolDetector::new();
        let result = detector.execute(json!({})).await;
        assert!(result.success);
        // 结果可能为空（取决于环境），但不应报错
        assert!(result.output["count"].is_number());
    }

    #[test]
    fn descriptor_has_correct_name() {
        let detector = ExternalToolDetector::new();
        assert_eq!(detector.name(), "tool_detector");
        assert_eq!(detector.descriptor().name, "tool_detector");
    }

    #[test]
    fn detect_nonexistent_tool() {
        let result = ExternalToolDetector::detect_tool("fake", "nonexistent_binary_xyz");
        assert!(result.is_none());
    }
}
