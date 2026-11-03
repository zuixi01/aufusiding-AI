//! 文件操作工具
//!
//! 在沙箱目录内提供安全的文件读写和目录列表功能。

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 文件操作工具 — 沙箱内文件读写
pub struct FileOpsTool {
    sandbox_root: PathBuf,
}

impl FileOpsTool {
    pub fn new(sandbox_root: PathBuf) -> Self {
        Self { sandbox_root }
    }

    /// 验证路径在沙箱内
    fn validate_path(&self, relative: &str) -> Result<PathBuf, String> {
        let target = self.sandbox_root.join(relative);
        // 使用 canonicalize 或手动检查防止路径遍历
        let normalized = if target.exists() {
            target
                .canonicalize()
                .map_err(|e| format!("路径解析失败: {}", e))?
        } else {
            // 文件不存在时检查父目录
            let parent = target.parent().unwrap_or(Path::new(""));
            if parent.exists() {
                let canon_parent = parent
                    .canonicalize()
                    .map_err(|e| format!("父目录解析失败: {}", e))?;
                let canon_sandbox = self
                    .sandbox_root
                    .canonicalize()
                    .map_err(|e| format!("沙箱根解析失败: {}", e))?;
                if !canon_parent.starts_with(&canon_sandbox) {
                    return Err("路径超出沙箱范围".to_string());
                }
            }
            target
        };

        let canon_sandbox = self
            .sandbox_root
            .canonicalize()
            .unwrap_or_else(|_| self.sandbox_root.clone());
        if !normalized.starts_with(&canon_sandbox) {
            return Err("路径超出沙箱范围".to_string());
        }
        Ok(normalized)
    }
}

#[async_trait]
impl AgentTool for FileOpsTool {
    fn name(&self) -> &str {
        "file_ops"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "file_ops".to_string(),
            description: "文件操作工具：读取、写入文件和列出目录内容".to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["read", "write", "list"] },
                    "path": { "type": "string", "description": "相对于沙箱根的路径" },
                    "content": { "type": "string", "description": "写入内容 (write 时必填)" }
                },
                "required": ["action", "path"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let action = match params.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("缺少 action 参数"),
        };
        let path = match params.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("缺少 path 参数"),
        };

        let full_path = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        match action {
            "read" => match std::fs::read_to_string(&full_path) {
                Ok(content) => ToolResult::ok(json!({ "content": content, "path": path })),
                Err(e) => ToolResult::err(format!("读取失败: {}", e)),
            },
            "write" => {
                let content = match params.get("content").and_then(|v| v.as_str()) {
                    Some(c) => c,
                    None => return ToolResult::err("write 操作需要 content 参数"),
                };
                if let Some(parent) = full_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&full_path, content) {
                    Ok(()) => ToolResult::ok(json!({ "written": true, "path": path })),
                    Err(e) => ToolResult::err(format!("写入失败: {}", e)),
                }
            }
            "list" => match std::fs::read_dir(&full_path) {
                Ok(entries) => {
                    let files: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect();
                    ToolResult::ok(json!({ "files": files, "count": files.len() }))
                }
                Err(e) => ToolResult::err(format!("列目录失败: {}", e)),
            },
            _ => ToolResult::err(format!("未知操作: {}", action)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn read_file_in_sandbox() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello").unwrap();
        let tool = FileOpsTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "read", "path": "test.txt"}))
            .await;
        assert!(result.success);
        assert_eq!(result.output["content"], "hello");
    }

    #[tokio::test]
    async fn write_file_in_sandbox() {
        let dir = tempdir().unwrap();
        let tool = FileOpsTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "write", "path": "out.txt", "content": "data"}))
            .await;
        assert!(result.success);
        let content = std::fs::read_to_string(dir.path().join("out.txt")).unwrap();
        assert_eq!(content, "data");
    }

    #[tokio::test]
    async fn list_directory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        let tool = FileOpsTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "list", "path": "."})).await;
        assert!(result.success);
        assert_eq!(result.output["count"], 2);
    }

    #[tokio::test]
    async fn reject_path_traversal() {
        let dir = tempdir().unwrap();
        let tool = FileOpsTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "read", "path": "../../etc/passwd"}))
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("沙箱"));
    }

    #[tokio::test]
    async fn read_nonexistent_file() {
        let dir = tempdir().unwrap();
        let tool = FileOpsTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({"action": "read", "path": "nope.txt"}))
            .await;
        assert!(!result.success);
    }
}
