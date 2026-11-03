//! 网页搜索工具
//!
//! 包装 BrowserCapability.research()，返回结构化搜索结果。
//! 实现 AgentTool trait。

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::tools::registry::{AgentTool, ToolDescriptor, ToolResult};

/// 搜索结果条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultEntry {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// 搜索后端 trait（解耦 BrowserCapability）
#[async_trait]
pub trait SearchBackend: Send + Sync {
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResultEntry>>;
}

/// Mock 搜索后端
pub struct MockSearchBackend;

#[async_trait]
impl SearchBackend for MockSearchBackend {
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResultEntry>> {
        let mut results = Vec::new();
        for i in 0..max_results.min(3) {
            results.push(SearchResultEntry {
                title: format!("Result {} for '{}'", i + 1, query),
                url: format!("https://example.com/search?q={}&p={}", query, i),
                snippet: format!("This is a snippet about '{}' from result {}.", query, i + 1),
            });
        }
        Ok(results)
    }
}

/// 网页搜索工具
pub struct WebSearchTool {
    backend: Arc<dyn SearchBackend>,
}

impl WebSearchTool {
    pub fn new(backend: Arc<dyn SearchBackend>) -> Self {
        Self { backend }
    }

    pub fn with_mock() -> Self {
        Self {
            backend: Arc::new(MockSearchBackend),
        }
    }
}

#[async_trait]
impl AgentTool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "web_search".into(),
            description: "搜索网页，返回标题、URL、摘要列表".into(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索关键词"},
                    "max_results": {"type": "integer", "default": 5}
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> ToolResult {
        let query = match params.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::err("missing 'query' parameter"),
        };
        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        match self.backend.search(query, max_results).await {
            Ok(results) => {
                let json_results: Vec<Value> = results
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "title": r.title,
                            "url": r.url,
                            "snippet": r.snippet,
                        })
                    })
                    .collect();
                ToolResult::ok(serde_json::json!({
                    "results": json_results,
                    "count": json_results.len(),
                }))
            }
            Err(e) => ToolResult::err(format!("search failed: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_basic_search() {
        let tool = WebSearchTool::with_mock();
        let result = tool
            .execute(serde_json::json!({
                "query": "Rust programming"
            }))
            .await;
        assert!(result.success);
        assert_eq!(result.output["count"], 3);
        let results = result.output["results"].as_array().unwrap();
        assert!(results[0]["title"]
            .as_str()
            .unwrap()
            .contains("Rust programming"));
    }

    #[tokio::test]
    async fn execute_with_max_results() {
        let tool = WebSearchTool::with_mock();
        let result = tool
            .execute(serde_json::json!({
                "query": "test",
                "max_results": 2
            }))
            .await;
        assert!(result.success);
        assert_eq!(result.output["count"], 2);
    }

    #[tokio::test]
    async fn execute_missing_query() {
        let tool = WebSearchTool::with_mock();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("missing"));
    }

    #[tokio::test]
    async fn search_result_has_url() {
        let tool = WebSearchTool::with_mock();
        let result = tool.execute(serde_json::json!({"query": "hello"})).await;
        assert!(result.success);
        let first = &result.output["results"][0];
        assert!(first["url"].as_str().unwrap().starts_with("https://"));
    }

    #[tokio::test]
    async fn search_result_has_snippet() {
        let tool = WebSearchTool::with_mock();
        let result = tool.execute(serde_json::json!({"query": "AI"})).await;
        assert!(result.success);
        let first = &result.output["results"][0];
        assert!(first["snippet"].as_str().unwrap().contains("AI"));
    }

    struct FailingBackend;

    #[async_trait]
    impl SearchBackend for FailingBackend {
        async fn search(
            &self,
            _query: &str,
            _max: usize,
        ) -> anyhow::Result<Vec<SearchResultEntry>> {
            anyhow::bail!("network error")
        }
    }

    #[tokio::test]
    async fn execute_backend_error() {
        let tool = WebSearchTool::new(Arc::new(FailingBackend));
        let result = tool.execute(serde_json::json!({"query": "test"})).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("network error"));
    }

    #[test]
    fn tool_name_and_descriptor() {
        let tool = WebSearchTool::with_mock();
        assert_eq!(tool.name(), "web_search");
        let desc = tool.descriptor();
        assert_eq!(desc.name, "web_search");
    }

    #[test]
    fn search_result_entry_serde_roundtrip() {
        let entry = SearchResultEntry {
            title: "Test".into(),
            url: "https://example.com".into(),
            snippet: "A test result".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: SearchResultEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "Test");
    }

    #[tokio::test]
    async fn mock_backend_direct() {
        let backend = MockSearchBackend;
        let results = backend.search("hello", 5).await.unwrap();
        assert_eq!(results.len(), 3); // mock caps at 3
    }

    #[tokio::test]
    async fn execute_default_max_results() {
        let tool = WebSearchTool::with_mock();
        let result = tool.execute(serde_json::json!({"query": "test"})).await;
        assert!(result.success);
        // mock returns min(max_results, 3), default max_results=5, so 3
        assert_eq!(result.output["count"], 3);
    }
}
