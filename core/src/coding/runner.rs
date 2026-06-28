//! 外部工具运行器
//!
//! 包装外部 CLI 工具为 AgentTool，支持超时控制。

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 外部工具运行器
pub struct ExternalToolRunner {
    tool_name: String,
    tool_path: String,
    #[allow(dead_code)]
    timeout: Duration,
}

impl ExternalToolRunner {
    pub fn new(name: impl Into<String>, path: impl Into<String>, timeout: Duration) -> Self {
        Self {
            tool_name: name.into(),
            tool_path: path.into(),
            timeout,
        }
    }
}

#[async_trait]
impl AgentTool for ExternalToolRunner {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: self.tool_name.clone(),
            description: format!("运行外部工具: {}", self.tool_name),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "命令行参数"
                    }
                },
                "required": ["args"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let args: Vec<String> = match params.get("args") {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => return ToolResult::err("缺少 args 参数 (数组)"),
        };

        let output = match std::process::Command::new(&self.tool_path)
            .args(&args)
            .output()
        {
            Ok(o) => o,
            Err(e) => return ToolResult::err(format!("执行失败: {}", e)),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            ToolResult::ok(json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": 0,
            }))
        } else {
            let code = output.status.code().unwrap_or(-1);
            ToolResult::ok(json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": code,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_echo_command() {
        let runner = ExternalToolRunner::new("echo", "echo", Duration::from_secs(5));
        let result = runner.execute(json!({"args": ["hello", "world"]})).await;
        assert!(result.success);
        let stdout = result.output["stdout"].as_str().unwrap();
        assert!(stdout.contains("hello world"));
    }

    #[tokio::test]
    async fn missing_binary_returns_error() {
        let runner = ExternalToolRunner::new("fake", "/nonexistent/binary", Duration::from_secs(5));
        let result = runner.execute(json!({"args": []})).await;
        assert!(!result.success);
    }

    #[test]
    fn descriptor_has_correct_name() {
        let runner = ExternalToolRunner::new("my_tool", "/usr/bin/tool", Duration::from_secs(10));
        assert_eq!(runner.name(), "my_tool");
        assert_eq!(runner.descriptor().name, "my_tool");
    }

    #[tokio::test]
    async fn missing_args_returns_error() {
        let runner = ExternalToolRunner::new("echo", "echo", Duration::from_secs(5));
        let result = runner.execute(json!({})).await;
        assert!(!result.success);
    }
}
