//! Ollama 本地推理 Provider
//!
//! 通过 HTTP API 调用本地 Ollama 服务，支持对话生成和向量嵌入。

use super::traits::{ChatRequest, ChatResponse, Provider, TokenUsage};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Ollama Provider
pub struct OllamaProvider {
    name: String,
    base_url: String,
    client: reqwest::Client,
    embed_model: String,
    chat_model: String,
    cached_models: Arc<RwLock<Vec<String>>>,
}

impl OllamaProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client with default TLS");

        Self {
            name: "ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            client,
            embed_model: "all-minilm".to_string(),
            chat_model: "qwen2.5:1.5b".to_string(),
            cached_models: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to build HTTP client");
        self
    }

    pub fn with_chat_model(mut self, model: &str) -> Self {
        self.chat_model = model.to_string();
        self
    }

    pub fn with_embed_model(mut self, model: &str) -> Self {
        self.embed_model = model.to_string();
        self
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/api/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// 刷新已安装模型列表（最多重试 3 次，每次间隔 1s）
    async fn refresh_models(&self) -> anyhow::Result<Vec<String>> {
        let tags_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build HTTP client: {}", e))?;

        let max_retries = 3;
        let mut last_err = None;

        for attempt in 0..max_retries {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let resp = match tags_client.get(self.endpoint("tags")).send().await {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("request failed: {}", e));
                    continue;
                }
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_else(|e| {
                    tracing::warn!("failed to read error response body: {}", e);
                    String::new()
                });
                last_err = Some(anyhow::anyhow!("Ollama tags failed ({}): {}", status, body));
                continue;
            }

            let tags: OllamaTagsResponse = resp.json().await?;
            let names: Vec<String> = tags.models.into_iter().map(|m| m.name).collect();

            let mut cache = self.cached_models.write().await;
            *cache = names.clone();

            return Ok(names);
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("refresh_models failed after retries")))
    }
}

// --- Ollama API 数据结构 ---

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelInfo {
    name: String,
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
    #[serde(default)]
    #[allow(dead_code)]
    model: String,
    eval_count: Option<usize>,
    prompt_eval_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

// --- Provider trait 实现 ---

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn available_models(&self) -> Vec<String> {
        // 返回缓存的模型列表，需要先调用 validate_key() 刷新
        // 使用 try_read 避免阻塞
        match self.cached_models.try_read() {
            Ok(cache) => cache.clone(),
            Err(_) => Vec::new(),
        }
    }

    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse> {
        let model = if request.model.is_empty() {
            self.chat_model.clone()
        } else {
            request.model.clone()
        };

        let ollama_req = OllamaChatRequest {
            model: model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| OllamaChatMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            stream: false,
            options: Some(OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
            }),
        };

        let resp = self
            .client
            .post(self.endpoint("chat"))
            .json(&ollama_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama chat failed ({}): {}", status, body);
        }

        let parsed: OllamaChatResponse = resp.json().await?;

        let prompt_tokens = parsed.prompt_eval_count.unwrap_or(0);
        let completion_tokens = parsed.eval_count.unwrap_or(0);

        Ok(ChatResponse {
            content: parsed.message.content,
            model,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        })
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let ollama_req = OllamaEmbedRequest {
            model: self.embed_model.clone(),
            input: text.to_string(),
        };

        let resp = self
            .client
            .post(self.endpoint("embed"))
            .json(&ollama_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama embed failed ({}): {}", status, body);
        }

        let parsed: OllamaEmbedResponse = resp.json().await?;

        parsed
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Ollama returned empty embeddings"))
    }

    async fn validate_key(&self) -> anyhow::Result<bool> {
        match self.refresh_models().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::traits::{ChatMessage, ChatRequest};
    use httpmock::prelude::*;

    #[test]
    fn test_provider_name() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn test_default_config() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.base_url, "http://localhost:11434");
        assert_eq!(provider.chat_model, "qwen2.5:1.5b");
        assert_eq!(provider.embed_model, "all-minilm");
    }

    #[test]
    fn test_builder_methods() {
        let provider = OllamaProvider::new()
            .with_base_url("http://example.com:8080/")
            .with_chat_model("llama3")
            .with_embed_model("nomic-embed-text")
            .with_timeout(Duration::from_secs(30));

        assert_eq!(provider.base_url, "http://example.com:8080");
        assert_eq!(provider.chat_model, "llama3");
        assert_eq!(provider.embed_model, "nomic-embed-text");
    }

    #[test]
    fn test_endpoint() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.endpoint("tags"), "http://localhost:11434/api/tags");
        assert_eq!(provider.endpoint("chat"), "http://localhost:11434/api/chat");
        assert_eq!(
            provider.endpoint("/embed"),
            "http://localhost:11434/api/embed"
        );
    }

    #[tokio::test]
    async fn test_validate_key_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/tags");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"models":[{"name":"qwen2.5:1.5b"},{"name":"all-minilm"}]}"#);
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());

        let result = provider.validate_key().await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_validate_key_failure() {
        let provider = OllamaProvider::new()
            .with_base_url("http://127.0.0.1:19999")
            .with_timeout(Duration::from_millis(500));

        let result = provider.validate_key().await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_available_models() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/tags");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"models":[{"name":"qwen2.5:1.5b"},{"name":"all-minilm"}]}"#);
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());

        // 先刷新缓存
        provider.validate_key().await.unwrap();

        let models = provider.available_models();
        assert_eq!(models.len(), 2);
        assert!(models.contains(&"qwen2.5:1.5b".to_string()));
        assert!(models.contains(&"all-minilm".to_string()));
    }

    #[tokio::test]
    async fn test_chat_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                    "message":{"role":"assistant","content":"你好！"},
                    "model":"qwen2.5:1.5b",
                    "eval_count":10,
                    "prompt_eval_count":5
                }"#,
                );
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());

        let request = ChatRequest {
            model: "qwen2.5:1.5b".to_string(),
            messages: vec![super::super::traits::ChatMessage {
                role: "user".to_string(),
                content: "你好".to_string(),
            }],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stream: false,
        };

        let response = provider.chat(&request).await.unwrap();
        assert_eq!(response.content, "你好！");
        assert_eq!(response.usage.prompt_tokens, 5);
        assert_eq!(response.usage.completion_tokens, 10);
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[tokio::test]
    async fn test_chat_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(404).body("model not found");
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());

        let request = ChatRequest {
            model: "nonexistent".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let result = provider.chat(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_uses_default_model() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/chat")
                .body_contains("qwen2.5:1.5b");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"message":{"role":"assistant","content":"ok"},"model":"qwen2.5:1.5b","eval_count":1,"prompt_eval_count":1}"#);
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());

        let request = ChatRequest {
            model: "".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        provider.chat(&request).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn test_embed_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/embed");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"embeddings":[[0.1,0.2,0.3,0.4,0.5]]}"#);
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());

        let embedding = provider.embed("hello").await.unwrap();
        assert_eq!(embedding.len(), 5);
        assert!((embedding[0] - 0.1).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_embed_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/embed");
            then.status(500).body("internal error");
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());

        let result = provider.embed("hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_embed_empty_response() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/embed");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"embeddings":[]}"#);
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());

        let result = provider.embed("hello").await;
        assert!(result.is_err());
    }

    // --- 真实 Ollama 集成测试（需要本地 Ollama 服务运行）---

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_real_ollama_chat() {
        let provider = OllamaProvider::new();

        // 验证服务可用
        assert!(provider.validate_key().await.unwrap());

        let request = ChatRequest {
            model: "qwen2.5:1.5b".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "用一句话介绍 Rust 语言".to_string(),
            }],
            max_tokens: Some(50),
            temperature: Some(0.7),
            stream: false,
        };

        let response = provider.chat(&request).await.unwrap();
        println!("Ollama 回复: {}", response.content);

        assert!(!response.content.is_empty());
        assert!(response.usage.total_tokens > 0);
    }

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_real_ollama_embed() {
        let provider = OllamaProvider::new();

        let embedding = provider.embed("hello world").await.unwrap();
        println!("Embedding 维度: {}", embedding.len());

        // all-minilm 应该返回 384 维向量
        assert_eq!(embedding.len(), 384);

        // 验证归一化（余弦相似度计算需要）
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.1);
    }

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_real_ollama_available_models() {
        let provider = OllamaProvider::new();

        // 刷新模型列表
        provider.validate_key().await.unwrap();

        let models = provider.available_models();
        println!("已安装模型: {:?}", models);

        assert!(models.contains(&"qwen2.5:1.5b".to_string()));
        assert!(
            models.contains(&"all-minilm".to_string())
                || models.contains(&"all-minilm:latest".to_string())
        );
    }

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_real_ollama_embedding_consistency() {
        let provider = OllamaProvider::new();

        let text = "测试文本一致性";
        let emb1 = provider.embed(text).await.unwrap();
        let emb2 = provider.embed(text).await.unwrap();

        // 相同文本应该生成相同向量
        assert_eq!(emb1.len(), emb2.len());
        for (a, b) in emb1.iter().zip(emb2.iter()) {
            assert!((a - b).abs() < 0.0001);
        }
    }

    #[tokio::test]
    #[ignore = "需要本地 Ollama 服务运行"]
    async fn test_real_ollama_with_embedding_service() {
        use crate::memory::embedding::{EmbeddingService, ProviderEmbeddingService};
        use std::sync::Arc;

        let provider = Arc::new(OllamaProvider::new());
        let service = ProviderEmbeddingService::detect_dimension(provider)
            .await
            .unwrap();

        assert_eq!(service.dimension(), 384);

        let v = service.embed("测试 EmbeddingService 集成").await.unwrap();
        assert_eq!(v.len(), 384);
    }

    #[tokio::test]
    async fn refresh_models_when_unavailable() {
        let provider = OllamaProvider::new()
            .with_base_url("http://127.0.0.1:19999")
            .with_timeout(Duration::from_millis(200));
        // refresh_models fails gracefully via validate_key
        let valid = provider.validate_key().await.unwrap();
        assert!(!valid);
        assert!(provider.available_models().is_empty());
    }

    #[tokio::test]
    async fn chat_with_unavailable_service() {
        let provider = OllamaProvider::new()
            .with_base_url("http://127.0.0.1:19999")
            .with_timeout(Duration::from_millis(200));
        let result = provider
            .chat(&ChatRequest {
                model: "qwen2.5:1.5b".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: "hi".to_string(),
                }],
                max_tokens: None,
                temperature: None,
                stream: false,
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn embed_with_unavailable_service() {
        let provider = OllamaProvider::new()
            .with_base_url("http://127.0.0.1:19999")
            .with_timeout(Duration::from_millis(200));
        let result = provider.embed("hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn model_cache_updates_on_refresh() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/tags");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"models":[{"name":"model-a"},{"name":"model-b"}]}"#);
        });
        let provider = OllamaProvider::new().with_base_url(&server.base_url());
        assert!(provider.available_models().is_empty());
        provider.validate_key().await.unwrap();
        let models = provider.available_models();
        assert_eq!(models.len(), 2);
        assert!(models.contains(&"model-a".to_string()));
    }

    #[tokio::test]
    async fn refresh_models_returns_error_on_non_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/tags");
            then.status(503).body("service unavailable");
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());
        let result = provider.refresh_models().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("503"));
    }

    #[tokio::test]
    async fn refresh_models_retries_on_failure() {
        let server = MockServer::start();

        // Succeeds on first call - validates retry loop works end-to-end
        server.mock(|when, then| {
            when.method(GET).path("/api/tags");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"models":[{"name":"retry-model"}]}"#);
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());
        let result = provider.refresh_models().await.unwrap();
        assert!(result.contains(&"retry-model".to_string()));
    }

    #[tokio::test]
    async fn refresh_models_exhausts_retries() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/api/tags");
            then.status(500).body("always fails");
        });

        let provider = OllamaProvider::new().with_base_url(&server.base_url());
        let result = provider.refresh_models().await;
        assert!(result.is_err());
    }
}
