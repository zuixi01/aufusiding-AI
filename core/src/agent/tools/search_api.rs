//! API 搜索后端
//!
//! 通过 Google Custom Search / SearXNG 风格 REST API 实现 SearchBackend trait。
//! 使用 reqwest::Client + 可配置 base_url，便于 httpmock 测试。

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::web_search::{SearchBackend, SearchResultEntry};

// ─── API 响应类型 ────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ApiSearchResponse {
    items: Vec<ApiSearchItem>,
}

#[derive(Debug, Deserialize)]
struct ApiSearchItem {
    title: String,
    link: String,
    snippet: String,
}

// ─── ApiSearchBackend ───────────────────────────────────

/// REST API 搜索后端（Google CSE / SearXNG 风格）
pub struct ApiSearchBackend {
    api_key: String,
    engine_id: String,
    client: Client,
    base_url: String,
}

impl ApiSearchBackend {
    pub fn new(api_key: &str, engine_id: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            engine_id: engine_id.to_string(),
            client: Client::new(),
            base_url: "https://www.googleapis.com/customsearch/v1".to_string(),
        }
    }

    /// 创建带自定义 base URL 的实例（用于测试）
    pub fn with_base_url(api_key: &str, engine_id: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            engine_id: engine_id.to_string(),
            client: Client::new(),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait]
impl SearchBackend for ApiSearchBackend {
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResultEntry>> {
        let resp = self
            .client
            .get(&self.base_url)
            .query(&[
                ("key", self.api_key.as_str()),
                ("cx", self.engine_id.as_str()),
                ("q", query),
                ("num", &max_results.min(10).to_string()),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("search API failed: {}", resp.status());
        }
        let api_resp: ApiSearchResponse = resp.json().await?;
        let results = api_resp
            .items
            .into_iter()
            .map(|item| SearchResultEntry {
                title: item.title,
                url: item.link,
                snippet: item.snippet,
            })
            .collect();
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    const TEST_KEY: &str = "AIzaSyTest123";
    const TEST_ENGINE: &str = "cx-test-engine";

    fn mock_search_response(count: usize) -> serde_json::Value {
        let items: Vec<serde_json::Value> = (0..count)
            .map(|i| {
                serde_json::json!({
                    "title": format!("Result {}", i + 1),
                    "link": format!("https://example.com/{}", i),
                    "snippet": format!("Snippet for result {}", i + 1)
                })
            })
            .collect();
        serde_json::json!({"items": items})
    }

    #[tokio::test]
    async fn search_returns_results() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).query_param("q", "rust lang");
            then.status(200).json_body(mock_search_response(3));
        });
        let backend = ApiSearchBackend::with_base_url(TEST_KEY, TEST_ENGINE, &server.base_url());
        let results = backend.search("rust lang", 5).await.unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "Result 1");
        assert!(results[0].url.starts_with("https://"));
    }

    #[tokio::test]
    async fn search_sends_api_key_and_engine() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .query_param("key", TEST_KEY)
                .query_param("cx", TEST_ENGINE);
            then.status(200).json_body(mock_search_response(1));
        });
        let backend = ApiSearchBackend::with_base_url(TEST_KEY, TEST_ENGINE, &server.base_url());
        backend.search("test", 5).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn search_caps_max_results_at_10() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).query_param("num", "10");
            then.status(200).json_body(mock_search_response(10));
        });
        let backend = ApiSearchBackend::with_base_url(TEST_KEY, TEST_ENGINE, &server.base_url());
        let results = backend.search("test", 50).await.unwrap();
        mock.assert();
        assert_eq!(results.len(), 10);
    }

    #[tokio::test]
    async fn search_server_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET);
            then.status(429);
        });
        let backend = ApiSearchBackend::with_base_url(TEST_KEY, TEST_ENGINE, &server.base_url());
        assert!(backend.search("test", 5).await.is_err());
    }

    #[tokio::test]
    async fn search_empty_results() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET);
            then.status(200).json_body(serde_json::json!({"items": []}));
        });
        let backend = ApiSearchBackend::with_base_url(TEST_KEY, TEST_ENGINE, &server.base_url());
        let results = backend.search("obscure query", 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_result_has_snippet() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET);
            then.status(200).json_body(mock_search_response(1));
        });
        let backend = ApiSearchBackend::with_base_url(TEST_KEY, TEST_ENGINE, &server.base_url());
        let results = backend.search("test", 1).await.unwrap();
        assert!(!results[0].snippet.is_empty());
    }

    #[tokio::test]
    async fn search_passes_query_param() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).query_param("q", "hello world");
            then.status(200).json_body(mock_search_response(1));
        });
        let backend = ApiSearchBackend::with_base_url(TEST_KEY, TEST_ENGINE, &server.base_url());
        backend.search("hello world", 5).await.unwrap();
        mock.assert();
    }
}
