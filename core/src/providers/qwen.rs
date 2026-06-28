//! 通义千问 API 适配器
//!
//! 支持阿里云通义千问系列模型。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Qwen Provider
pub struct QwenProvider {
    /// API 密钥
    api_key: String,
    /// API 端点
    endpoint: String,
    client: reqwest::Client,
}

impl QwenProvider {
    /// 创建 Qwen Provider
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            endpoint: "https://dashscope.aliyuncs.com/api/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.endpoint = url.trim_end_matches('/').to_string();
        self
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.endpoint, path.trim_start_matches('/'))
    }
}

#[async_trait]
impl super::traits::Provider for QwenProvider {
    fn name(&self) -> &str {
        "qwen"
    }

    fn available_models(&self) -> Vec<String> {
        vec![
            "qwen-turbo".to_string(),
            "qwen-plus".to_string(),
            "qwen-max".to_string(),
        ]
    }

    async fn chat(
        &self,
        request: &super::traits::ChatRequest,
    ) -> anyhow::Result<super::traits::ChatResponse> {
        let payload = QwenChatRequest {
            model: request.model.clone(),
            input: QwenChatInput {
                messages: request
                    .messages
                    .iter()
                    .map(|m| QwenMessage {
                        role: m.role.clone(),
                        content: m.content.clone(),
                    })
                    .collect(),
            },
            parameters: QwenChatParameters {
                max_tokens: request.max_tokens,
                temperature: request.temperature,
            },
        };

        let response = self
            .client
            .post(self.endpoint("services/aigc/text-generation/generation"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read qwen chat error body: {}", e);
                String::new()
            });
            anyhow::bail!("qwen chat failed: status={}, body={}", status, body);
        }

        let parsed: QwenChatResponse = response.json().await?;
        let content = parsed.output.text;

        Ok(super::traits::ChatResponse {
            content,
            model: parsed.model,
            usage: super::traits::TokenUsage {
                prompt_tokens: parsed.usage.input_tokens,
                completion_tokens: parsed.usage.output_tokens,
                total_tokens: parsed.usage.total_tokens,
            },
        })
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let payload = QwenEmbeddingRequest {
            model: "text-embedding-v3".to_string(),
            input: QwenEmbeddingInput {
                texts: vec![text.to_string()],
            },
        };

        let response = self
            .client
            .post(self.endpoint("services/embeddings/text-embedding/text-embedding"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read qwen embedding error body: {}", e);
                String::new()
            });
            anyhow::bail!("qwen embedding failed: status={}, body={}", status, body);
        }

        let parsed: QwenEmbeddingResponse = response.json().await?;
        let embedding = parsed
            .output
            .embeddings
            .first()
            .map(|e| e.embedding.clone())
            .unwrap_or_default();

        Ok(embedding)
    }

    async fn validate_key(&self) -> anyhow::Result<bool> {
        if self.api_key.trim().is_empty() {
            return Ok(false);
        }

        let response = self
            .client
            .get(self.endpoint("models"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("qwen key validation request failed: {}", e))?;

        let status = response.status().as_u16();
        match status {
            200 => Ok(true),
            401 => Ok(false),
            _ => {
                let body = response.text().await.unwrap_or_else(|e| {
                    tracing::warn!("failed to read qwen validation error body: {}", e);
                    String::new()
                });
                anyhow::bail!(
                    "qwen key validation unexpected status={}, body={}",
                    status,
                    body
                )
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct QwenChatRequest {
    model: String,
    input: QwenChatInput,
    parameters: QwenChatParameters,
}

#[derive(Debug, Serialize)]
struct QwenChatInput {
    messages: Vec<QwenMessage>,
}

#[derive(Debug, Serialize)]
struct QwenChatParameters {
    max_tokens: Option<usize>,
    temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct QwenMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct QwenChatResponse {
    model: String,
    output: QwenChatOutput,
    usage: QwenUsage,
}

#[derive(Debug, Deserialize)]
struct QwenChatOutput {
    text: String,
}

#[derive(Debug, Deserialize)]
struct QwenUsage {
    input_tokens: usize,
    output_tokens: usize,
    total_tokens: usize,
}

#[derive(Debug, Serialize)]
struct QwenEmbeddingRequest {
    model: String,
    input: QwenEmbeddingInput,
}

#[derive(Debug, Serialize)]
struct QwenEmbeddingInput {
    texts: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct QwenEmbeddingResponse {
    output: QwenEmbeddingOutput,
}

#[derive(Debug, Deserialize)]
struct QwenEmbeddingOutput {
    embeddings: Vec<QwenEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct QwenEmbeddingData {
    embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::traits::{ChatMessage, ChatRequest, Provider};
    use httpmock::prelude::*;

    #[tokio::test]
    async fn validate_key_checks_generation_endpoint() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/models")
                .header("authorization", "Bearer test-key");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"data":[]}"#);
        });

        let provider = QwenProvider::new("test-key").with_base_url(&server.base_url());
        let valid = provider.validate_key().await.unwrap();
        assert!(valid);
        mock.assert();
    }

    #[tokio::test]
    async fn chat_parses_qwen_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/aigc/text-generation/generation")
                .header("authorization", "Bearer test-key");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "model":"qwen-plus",
                        "output":{"text":"你好，我是千问"},
                        "usage":{"input_tokens":10,"output_tokens":6,"total_tokens":16}
                    }"#,
                );
        });

        let provider = QwenProvider::new("test-key").with_base_url(&server.base_url());
        let response = provider
            .chat(&ChatRequest {
                model: "qwen-plus".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: "你好".to_string(),
                }],
                max_tokens: Some(64),
                temperature: Some(0.3),
                stream: false,
            })
            .await
            .unwrap();

        assert_eq!(response.content, "你好，我是千问");
        assert_eq!(response.model, "qwen-plus");
        assert_eq!(response.usage.total_tokens, 16);
        mock.assert();
    }

    #[tokio::test]
    async fn embed_parses_qwen_embedding_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/services/embeddings/text-embedding/text-embedding")
                .header("authorization", "Bearer test-key");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"output":{"embeddings":[{"embedding":[0.5,0.6]}]}}"#);
        });

        let provider = QwenProvider::new("test-key").with_base_url(&server.base_url());
        let embedding = provider.embed("hello").await.unwrap();
        assert_eq!(embedding, vec![0.5, 0.6]);
        mock.assert();
    }

    #[tokio::test]
    async fn chat_returns_error_on_non_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/services/aigc/text-generation/generation");
            then.status(401).body("unauthorized");
        });
        let provider = QwenProvider::new("bad-key").with_base_url(&server.base_url());
        let result = provider
            .chat(&ChatRequest {
                model: "qwen-turbo".to_string(),
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
    async fn validate_key_empty_returns_false() {
        let provider = QwenProvider::new("");
        let valid = provider.validate_key().await.unwrap();
        assert!(!valid);
    }

    #[tokio::test]
    async fn chat_rate_limit_429_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/services/aigc/text-generation/generation");
            then.status(429).body("rate limited");
        });
        let provider = QwenProvider::new("key").with_base_url(&server.base_url());
        let result = provider
            .chat(&ChatRequest {
                model: "qwen-turbo".to_string(),
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
        assert!(result.unwrap_err().to_string().contains("429"));
    }

    #[tokio::test]
    async fn embed_returns_error_on_non_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/services/embeddings/text-embedding/text-embedding");
            then.status(500).body("internal error");
        });
        let provider = QwenProvider::new("key").with_base_url(&server.base_url());
        let result = provider.embed("hello").await;
        assert!(result.is_err());
    }
}
