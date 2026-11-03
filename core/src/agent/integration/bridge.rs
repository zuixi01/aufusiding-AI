//! 工具桥接器 — 将现有模块包装为 Agent 工具
//!
//! 将 MemoryManager、PluginManager 等现有模块包装为 `AgentTool` trait 实现，
//! 供 Agent 在 ReAct 循环中调用。

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

// ─── MemoryTool ─────────────────────────────────────────

/// 记忆检索工具 — 包装 MemoryManager
pub struct MemoryTool {
    manager: Arc<RwLock<crate::memory::manager::MemoryManager>>,
}

impl MemoryTool {
    pub fn new(manager: Arc<RwLock<crate::memory::manager::MemoryManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl AgentTool for MemoryTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "memory_search".into(),
            description: "搜索用户记忆和历史上下文，返回相关记忆片段".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索关键词" },
                    "max_items": { "type": "integer", "description": "最大返回条数" }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let max_items = params
            .get("max_items")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let mm = self.manager.read().await;
        let context = mm.build_memory_context(query, max_items).await;
        if context.is_empty() {
            ToolResult::ok(serde_json::json!({"results": [], "message": "无相关记忆"}))
        } else {
            ToolResult::ok(serde_json::json!({"results": [context]}))
        }
    }
}

// ─── PluginTool ─────────────────────────────────────────

/// 插件执行工具 — 包装 PluginManager
pub struct PluginTool {
    manager: Arc<RwLock<crate::plugins::manager::PluginManager>>,
}

impl PluginTool {
    pub fn new(manager: Arc<RwLock<crate::plugins::manager::PluginManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl AgentTool for PluginTool {
    fn name(&self) -> &str {
        "plugin_execute"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "plugin_execute".into(),
            description: "执行已注册的插件，传入插件ID和输入参数".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "plugin_id": { "type": "string", "description": "插件ID" },
                    "input": { "type": "string", "description": "插件输入" }
                },
                "required": ["plugin_id", "input"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let plugin_id = match params.get("plugin_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("缺少 plugin_id 参数"),
        };
        let input = params.get("input").and_then(|v| v.as_str()).unwrap_or("");

        let mgr = self.manager.read().await;
        match mgr.execute(plugin_id, input) {
            Ok(output) => ToolResult::ok(serde_json::json!({"output": output})),
            Err(e) => ToolResult::err(e),
        }
    }
}

// ─── CodeSearchTool ─────────────────────────────────────

/// 代码搜索工具 — 在给定代码片段中搜索关键词
pub struct CodeSearchTool {
    /// 可搜索的代码片段库
    snippets: Arc<RwLock<Vec<CodeSnippet>>>,
}

/// 代码片段
#[derive(Debug, Clone)]
pub struct CodeSnippet {
    pub file_path: String,
    pub content: String,
}

impl CodeSearchTool {
    pub fn new() -> Self {
        Self {
            snippets: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_snippet(&self, file_path: impl Into<String>, content: impl Into<String>) {
        let mut snippets = self.snippets.write().await;
        snippets.push(CodeSnippet {
            file_path: file_path.into(),
            content: content.into(),
        });
    }
}

#[async_trait]
impl AgentTool for CodeSearchTool {
    fn name(&self) -> &str {
        "code_search"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "code_search".into(),
            description: "在代码库中搜索关键词，返回匹配的文件和代码片段".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "keyword": { "type": "string", "description": "搜索关键词" }
                },
                "required": ["keyword"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let keyword = match params.get("keyword").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return ToolResult::err("缺少 keyword 参数"),
        };

        let snippets = self.snippets.read().await;
        let matches: Vec<Value> = snippets
            .iter()
            .filter(|s| s.content.contains(keyword) || s.file_path.contains(keyword))
            .map(|s| {
                serde_json::json!({
                    "file": s.file_path,
                    "preview": s.content.chars().take(200).collect::<String>()
                })
            })
            .collect();

        ToolResult::ok(serde_json::json!({
            "matches": matches,
            "count": matches.len()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::registry::ToolRegistry;

    #[tokio::test]
    async fn code_search_finds_matches() {
        let tool = CodeSearchTool::new();
        tool.add_snippet("src/main.rs", "fn main() { println!(\"hello\"); }")
            .await;
        tool.add_snippet("src/lib.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }")
            .await;

        let result = tool.execute(serde_json::json!({"keyword": "main"})).await;
        assert!(result.success);
        assert_eq!(result.output["count"], 1);
    }

    #[tokio::test]
    async fn code_search_no_matches() {
        let tool = CodeSearchTool::new();
        tool.add_snippet("src/lib.rs", "pub fn add() {}").await;

        let result = tool
            .execute(serde_json::json!({"keyword": "nonexistent"}))
            .await;
        assert!(result.success);
        assert_eq!(result.output["count"], 0);
    }

    #[tokio::test]
    async fn code_search_missing_keyword() {
        let tool = CodeSearchTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn tools_register_to_registry() {
        let mut registry = ToolRegistry::new();
        let code_tool = Arc::new(CodeSearchTool::new());
        registry.register(code_tool);
        assert_eq!(registry.len(), 1);
        assert!(registry.get("code_search").is_some());
    }
}
