//! 文档工具
//!
//! 创建 Markdown 文档、结构化报告，支持追加/替换修改。
//! 实现 AgentTool trait。

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 文档工具
pub struct DocumentTool;

impl DocumentTool {
    pub fn new() -> Self {
        Self
    }

    /// 创建 Markdown 文档
    pub fn create_markdown(title: &str, body: &str) -> String {
        format!("# {}\n\n{}", title, body)
    }

    /// 创建结构化报告
    pub fn create_report(title: &str, sections: &[(String, String)]) -> String {
        let mut doc = format!("# {}\n\n", title);
        for (heading, content) in sections {
            doc.push_str(&format!("## {}\n\n{}\n\n", heading, content));
        }
        doc.trim_end().to_string()
    }

    /// 修改文档：追加
    pub fn append(document: &str, content: &str) -> String {
        format!("{}\n\n{}", document, content)
    }

    /// 修改文档：替换
    pub fn replace(document: &str, search: &str, replacement: &str) -> String {
        document.replace(search, replacement)
    }
}

#[async_trait]
impl AgentTool for DocumentTool {
    fn name(&self) -> &str {
        "document"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "document".into(),
            description: "创建和修改 Markdown 文档、结构化报告".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["create_markdown", "create_report", "modify"]},
                    "title": {"type": "string"},
                    "body": {"type": "string"},
                    "sections": {"type": "array", "items": {"type": "object", "properties": {"heading": {"type": "string"}, "content": {"type": "string"}}}},
                    "document": {"type": "string"},
                    "modify_action": {"type": "string", "enum": ["append", "replace"]},
                    "content": {"type": "string"},
                    "search": {"type": "string"},
                    "replacement": {"type": "string"}
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let action = match params.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("missing 'action' parameter"),
        };

        match action {
            "create_markdown" => {
                let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
                let body = params.get("body").and_then(|v| v.as_str()).unwrap_or("");
                let doc = Self::create_markdown(title, body);
                ToolResult::ok(serde_json::json!({"document": doc}))
            }
            "create_report" => {
                let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("Report");
                let sections: Vec<(String, String)> = params.get("sections")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter().map(|s| {
                            let h = s.get("heading").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let c = s.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            (h, c)
                        }).collect()
                    })
                    .unwrap_or_default();
                let doc = Self::create_report(title, &sections);
                ToolResult::ok(serde_json::json!({"document": doc}))
            }
            "modify" => {
                let document = match params.get("document").and_then(|v| v.as_str()) {
                    Some(d) => d,
                    None => return ToolResult::err("missing 'document' for modify"),
                };
                let modify_action = params.get("modify_action").and_then(|v| v.as_str()).unwrap_or("append");
                let result = match modify_action {
                    "append" => {
                        let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        Self::append(document, content)
                    }
                    "replace" => {
                        let search = params.get("search").and_then(|v| v.as_str()).unwrap_or("");
                        let replacement = params.get("replacement").and_then(|v| v.as_str()).unwrap_or("");
                        Self::replace(document, search, replacement)
                    }
                    _ => return ToolResult::err(format!("unknown modify_action: {}", modify_action)),
                };
                ToolResult::ok(serde_json::json!({"document": result}))
            }
            _ => ToolResult::err(format!("unknown action: {}", action)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_markdown_basic() {
        let doc = DocumentTool::create_markdown("标题", "正文内容");
        assert!(doc.starts_with("# 标题"));
        assert!(doc.contains("正文内容"));
    }

    #[test]
    fn create_report_with_sections() {
        let sections = vec![
            ("概述".into(), "项目概述内容".into()),
            ("结论".into(), "最终结论".into()),
        ];
        let doc = DocumentTool::create_report("季度报告", &sections);
        assert!(doc.contains("# 季度报告"));
        assert!(doc.contains("## 概述"));
        assert!(doc.contains("## 结论"));
    }

    #[test]
    fn create_report_empty_sections() {
        let doc = DocumentTool::create_report("空报告", &[]);
        assert!(doc.contains("# 空报告"));
    }

    #[test]
    fn append_content() {
        let doc = DocumentTool::create_markdown("标题", "段落一");
        let updated = DocumentTool::append(&doc, "段落二");
        assert!(updated.contains("段落一"));
        assert!(updated.contains("段落二"));
    }

    #[test]
    fn replace_content() {
        let doc = "Hello World";
        let updated = DocumentTool::replace(doc, "World", "Rust");
        assert_eq!(updated, "Hello Rust");
    }

    #[test]
    fn replace_no_match() {
        let doc = "Hello World";
        let updated = DocumentTool::replace(doc, "Python", "Rust");
        assert_eq!(updated, "Hello World");
    }

    #[tokio::test]
    async fn execute_create_markdown() {
        let tool = DocumentTool::new();
        let result = tool.execute(serde_json::json!({
            "action": "create_markdown",
            "title": "测试",
            "body": "内容"
        })).await;
        assert!(result.success);
        assert!(result.output["document"].as_str().unwrap().contains("# 测试"));
    }

    #[tokio::test]
    async fn execute_create_report() {
        let tool = DocumentTool::new();
        let result = tool.execute(serde_json::json!({
            "action": "create_report",
            "title": "报告",
            "sections": [{"heading": "摘要", "content": "内容"}]
        })).await;
        assert!(result.success);
        let doc = result.output["document"].as_str().unwrap();
        assert!(doc.contains("## 摘要"));
    }

    #[tokio::test]
    async fn execute_modify_append() {
        let tool = DocumentTool::new();
        let result = tool.execute(serde_json::json!({
            "action": "modify",
            "document": "原始内容",
            "modify_action": "append",
            "content": "新增内容"
        })).await;
        assert!(result.success);
        let doc = result.output["document"].as_str().unwrap();
        assert!(doc.contains("原始内容"));
        assert!(doc.contains("新增内容"));
    }

    #[tokio::test]
    async fn execute_modify_replace() {
        let tool = DocumentTool::new();
        let result = tool.execute(serde_json::json!({
            "action": "modify",
            "document": "Hello World",
            "modify_action": "replace",
            "search": "World",
            "replacement": "Rust"
        })).await;
        assert!(result.success);
        assert_eq!(result.output["document"], "Hello Rust");
    }

    #[tokio::test]
    async fn execute_missing_action() {
        let tool = DocumentTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn execute_unknown_action() {
        let tool = DocumentTool::new();
        let result = tool.execute(serde_json::json!({"action": "delete"})).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn execute_modify_missing_document() {
        let tool = DocumentTool::new();
        let result = tool.execute(serde_json::json!({
            "action": "modify",
            "modify_action": "append",
            "content": "test"
        })).await;
        assert!(!result.success);
    }

    #[test]
    fn tool_name_and_descriptor() {
        let tool = DocumentTool::new();
        assert_eq!(tool.name(), "document");
        let desc = tool.descriptor();
        assert_eq!(desc.name, "document");
    }
}
