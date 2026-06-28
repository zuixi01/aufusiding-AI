//! 工具注册表 — 统一管理 Agent 可用的工具

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: Value,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(output: Value) -> Self {
        Self {
            success: true,
            output,
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: Value::Null,
            error: Some(message.into()),
        }
    }
}

/// 工具描述符 — 用于 LLM function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
}

/// Agent 工具 trait — 所有工具的统一接口
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;

    /// 工具描述（供 LLM 理解用途）
    fn descriptor(&self) -> ToolDescriptor;

    /// 执行工具
    async fn execute(&self, params: Value) -> ToolResult;
}

/// 工具注册表 — 管理所有可用工具
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册工具
    pub fn register(&mut self, tool: Arc<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// 注销工具
    pub fn unregister(&mut self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.remove(name)
    }

    /// 获取工具
    pub fn get(&self, name: &str) -> Option<&Arc<dyn AgentTool>> {
        self.tools.get(name)
    }

    /// 执行指定工具
    pub async fn execute(&self, name: &str, params: Value) -> ToolResult {
        match self.tools.get(name) {
            Some(tool) => tool.execute(params).await,
            None => ToolResult::err(format!("tool not found: {}", name)),
        }
    }

    /// 列出所有工具描述符（供 LLM function calling）
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools.values().map(|t| t.descriptor()).collect()
    }

    /// 工具名称列表
    pub fn list_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// 工具数量
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// Mock 工具（用于测试）
pub struct MockTool {
    tool_name: String,
    response: Value,
}

impl MockTool {
    pub fn new(name: impl Into<String>, response: Value) -> Self {
        Self {
            tool_name: name.into(),
            response,
        }
    }
}

#[async_trait]
impl AgentTool for MockTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: self.tool_name.clone(),
            description: format!("Mock tool: {}", self.tool_name),
            parameters_schema: serde_json::json!({"type": "object"}),
        }
    }

    async fn execute(&self, _params: Value) -> ToolResult {
        ToolResult::ok(self.response.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_tool(name: &str) -> Arc<dyn AgentTool> {
        Arc::new(MockTool::new(name, serde_json::json!({"status": "ok"})))
    }

    #[test]
    fn register_and_get() {
        let mut reg = ToolRegistry::new();
        reg.register(mock_tool("search"));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("search").is_some());
        assert!(reg.get("nope").is_none());
    }

    #[test]
    fn unregister_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(mock_tool("search"));
        reg.unregister("search");
        assert!(reg.is_empty());
    }

    #[tokio::test]
    async fn execute_existing_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(mock_tool("search"));
        let result = reg.execute("search", Value::Null).await;
        assert!(result.success);
    }

    #[tokio::test]
    async fn execute_missing_tool() {
        let reg = ToolRegistry::new();
        let result = reg.execute("nope", Value::Null).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[test]
    fn descriptors_lists_all() {
        let mut reg = ToolRegistry::new();
        reg.register(mock_tool("search"));
        reg.register(mock_tool("code"));
        let descs = reg.descriptors();
        assert_eq!(descs.len(), 2);
    }

    #[test]
    fn tool_result_constructors() {
        let ok = ToolResult::ok(serde_json::json!("done"));
        assert!(ok.success);
        assert!(ok.error.is_none());

        let err = ToolResult::err("failed");
        assert!(!err.success);
        assert_eq!(err.error, Some("failed".into()));
    }
}
