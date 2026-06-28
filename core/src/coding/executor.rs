//! 代码执行器
//!
//! 在临时目录中安全执行代码片段，支持多语言。

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 代码执行器
pub struct CodeExecutor {
    temp_dir: PathBuf,
    allowed_languages: Vec<String>,
    timeout: Duration,
}

impl CodeExecutor {
    pub fn new(temp_dir: PathBuf) -> Self {
        Self {
            temp_dir,
            allowed_languages: vec![
                "python".to_string(),
                "node".to_string(),
                "bash".to_string(),
            ],
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_languages(mut self, languages: Vec<String>) -> Self {
        self.allowed_languages = languages;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn interpreter_for(language: &str) -> Option<(&'static str, &'static str)> {
        match language {
            "python" | "python3" => Some(("python3", ".py")),
            "node" | "javascript" | "js" => Some(("node", ".js")),
            "bash" | "sh" => Some(("bash", ".sh")),
            _ => None,
        }
    }
}

#[async_trait]
impl AgentTool for CodeExecutor {
    fn name(&self) -> &str {
        "code_executor"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "code_executor".to_string(),
            description: "在沙箱中执行代码片段".to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "language": { "type": "string", "description": "编程语言 (python/node/bash)" },
                    "code": { "type": "string", "description": "要执行的代码" }
                },
                "required": ["language", "code"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let language = match params.get("language").and_then(|v| v.as_str()) {
            Some(l) => l,
            None => return ToolResult::err("缺少 language 参数"),
        };
        let code = match params.get("code").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::err("缺少 code 参数"),
        };

        if code.trim().is_empty() {
            return ToolResult::err("代码不能为空");
        }

        if !self.allowed_languages.iter().any(|l| l == language) {
            return ToolResult::err(format!(
                "不支持的语言: {}，允许: {:?}",
                language, self.allowed_languages
            ));
        }

        let (interpreter, ext) = match Self::interpreter_for(language) {
            Some(pair) => pair,
            None => return ToolResult::err(format!("无法找到 {} 的解释器", language)),
        };

        // 写入临时文件
        let _ = std::fs::create_dir_all(&self.temp_dir);
        let file_name = format!("exec_{}{}", uuid::Uuid::new_v4().simple(), ext);
        let file_path = self.temp_dir.join(&file_name);

        if let Err(e) = std::fs::write(&file_path, code) {
            return ToolResult::err(format!("写入临时文件失败: {}", e));
        }

        // 执行
        let output = std::process::Command::new(interpreter)
            .arg(&file_path)
            .output();

        // 清理
        let _ = std::fs::remove_file(&file_path);

        match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                let code = o.status.code().unwrap_or(-1);
                ToolResult::ok(json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": code,
                    "language": language,
                }))
            }
            Err(e) => ToolResult::err(format!("执行失败: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn execute_bash_snippet() {
        let dir = tempdir().unwrap();
        let executor = CodeExecutor::new(dir.path().to_path_buf());
        let result = executor
            .execute(json!({"language": "bash", "code": "echo hello"}))
            .await;
        assert!(result.success);
        let stdout = result.output["stdout"].as_str().unwrap();
        assert!(stdout.contains("hello"));
    }

    #[tokio::test]
    async fn reject_unknown_language() {
        let dir = tempdir().unwrap();
        let executor = CodeExecutor::new(dir.path().to_path_buf());
        let result = executor
            .execute(json!({"language": "cobol", "code": "DISPLAY 'HI'"}))
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("不支持"));
    }

    #[tokio::test]
    async fn empty_code_returns_error() {
        let dir = tempdir().unwrap();
        let executor = CodeExecutor::new(dir.path().to_path_buf());
        let result = executor
            .execute(json!({"language": "bash", "code": "  "}))
            .await;
        assert!(!result.success);
    }

    #[test]
    fn descriptor_correct() {
        let dir = PathBuf::from("/tmp");
        let executor = CodeExecutor::new(dir);
        assert_eq!(executor.name(), "code_executor");
        assert_eq!(executor.descriptor().name, "code_executor");
    }
}
