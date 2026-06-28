//! 项目搜索工具
//!
//! 在项目目录中递归搜索匹配正则表达式的代码行。

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 搜索匹配结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub file: String,
    pub line_number: usize,
    pub content: String,
}

/// 项目搜索工具
pub struct ProjectSearchTool {
    project_root: PathBuf,
    max_results: usize,
}

impl ProjectSearchTool {
    pub fn new(project_root: PathBuf, max_results: usize) -> Self {
        Self { project_root, max_results }
    }
}

#[async_trait]
impl AgentTool for ProjectSearchTool {
    fn name(&self) -> &str { "project_search" }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "project_search".to_string(),
            description: "在项目中搜索匹配正则表达式的代码行".to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "正则表达式" },
                    "file_glob": { "type": "string", "description": "文件过滤 (如 *.rs)" }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let pattern = match params.get("pattern").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("缺少 pattern 参数"),
        };
        let file_glob = params.get("file_glob").and_then(|v| v.as_str());

        let regex = match regex::Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("无效正则表达式: {}", e)),
        };

        let mut matches = Vec::new();
        self.search_dir(&self.project_root, &regex, file_glob, &mut matches);

        let count = matches.len();
        ToolResult::ok(json!({ "matches": matches, "count": count }))
    }
}

impl ProjectSearchTool {
    fn search_dir(
        &self,
        dir: &std::path::Path,
        regex: &regex::Regex,
        file_glob: Option<&str>,
        matches: &mut Vec<SearchMatch>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            if matches.len() >= self.max_results {
                return;
            }
            let path = entry.path();
            if path.is_dir() {
                self.search_dir(&path, regex, file_glob, matches);
            } else if path.is_file() {
                if let Some(glob) = file_glob {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if !Self::matches_glob(&name, glob) {
                        continue;
                    }
                }
                self.search_file(&path, regex, matches);
            }
        }
    }

    fn search_file(
        &self,
        path: &std::path::Path,
        regex: &regex::Regex,
        matches: &mut Vec<SearchMatch>,
    ) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let rel = path.strip_prefix(&self.project_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        for (i, line) in content.lines().enumerate() {
            if matches.len() >= self.max_results {
                return;
            }
            if regex.is_match(line) {
                matches.push(SearchMatch {
                    file: rel.clone(),
                    line_number: i + 1,
                    content: line.to_string(),
                });
            }
        }
    }

    fn matches_glob(name: &str, glob: &str) -> bool {
        if let Some(ext) = glob.strip_prefix("*.") {
            name.ends_with(&format!(".{}", ext))
        } else {
            name.contains(glob)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn search_finds_pattern() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn hello() {}\nfn world() {}").unwrap();
        let tool = ProjectSearchTool::new(dir.path().to_path_buf(), 100);
        let result = tool.execute(json!({"pattern": "hello"})).await;
        assert!(result.success);
        assert_eq!(result.output["count"], 1);
    }

    #[tokio::test]
    async fn search_respects_glob() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "target line").unwrap();
        std::fs::write(dir.path().join("b.txt"), "target line").unwrap();
        let tool = ProjectSearchTool::new(dir.path().to_path_buf(), 100);
        let result = tool.execute(json!({"pattern": "target", "file_glob": "*.rs"})).await;
        assert!(result.success);
        assert_eq!(result.output["count"], 1);
    }

    #[tokio::test]
    async fn search_no_matches() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "nothing here").unwrap();
        let tool = ProjectSearchTool::new(dir.path().to_path_buf(), 100);
        let result = tool.execute(json!({"pattern": "zzzzz"})).await;
        assert!(result.success);
        assert_eq!(result.output["count"], 0);
    }

    #[tokio::test]
    async fn search_invalid_regex() {
        let dir = tempdir().unwrap();
        let tool = ProjectSearchTool::new(dir.path().to_path_buf(), 100);
        let result = tool.execute(json!({"pattern": "[invalid"})).await;
        assert!(!result.success);
    }
}
