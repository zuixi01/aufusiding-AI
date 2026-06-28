//! OpenAI API 适配器
//!
//! 支持 GPT-4o、GPT-4o-mini 等 OpenAI 模型，兼容 OpenAI API 格式的第三方服务。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// OpenAI Provider
pub struct OpenAiProvider {
    /// API 密钥
    api_key: String,
    /// API 基础 URL（支持自定义端点）
    base_url: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    /// 创建 OpenAI Provider
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// 使用自定义端点（兼容 API）
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

#[async_trait]
impl super::traits::Provider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn available_models(&self) -> Vec<String> {
        vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()]
    }

    async fn chat(
        &self,
        request: &super::traits::ChatRequest,
    ) -> anyhow::Result<super::traits::ChatResponse> {
        let payload = OpenAiChatRequest {
            model: request.model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| OpenAiMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: request.stream,
        };

        let response = self
            .client
            .post(self.endpoint("chat/completions"))
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read openai chat error body: {}", e);
                String::new()
            });
            anyhow::bail!("openai chat failed: status={}, body={}", status, body);
        }

        let parsed: OpenAiChatResponse = response.json().await?;
        let content = parsed
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        let usage = parsed.usage.unwrap_or_default();

        Ok(super::traits::ChatResponse {
            content,
            model: parsed.model,
            usage: super::traits::TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            },
        })
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let payload = OpenAiEmbeddingRequest {
            model: "text-embedding-3-small".to_string(),
            input: text.to_string(),
        };

        let response = self
            .client
            .post(self.endpoint("embeddings"))
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read openai embedding error body: {}", e);
                String::new()
            });
            anyhow::bail!("openai embedding failed: status={}, body={}", status, body);
        }

        let parsed: OpenAiEmbeddingResponse = response.json().await?;
        let embedding = parsed
            .data
            .first()
            .map(|d| d.embedding.clone())
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
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("openai key validation request failed: {}", e))?;

        let status = response.status().as_u16();
        match status {
            200 => Ok(true),
            401 => Ok(false),
            _ => {
                let body = response.text().await.unwrap_or_else(|e| {
                    tracing::warn!("failed to read openai validation error body: {}", e);
                    String::new()
                });
                anyhow::bail!(
                    "openai key validation unexpected status={}, body={}",
                    status,
                    body
                )
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    max_tokens: Option<usize>,
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAiUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[derive(Debug, Serialize)]
struct OpenAiEmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::traits::{ChatMessage, ChatRequest, Provider};
    use httpmock::prelude::*;

    #[tokio::test]
    async fn validate_key_checks_models_endpoint() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/models")
                .header("authorization", "Bearer test-key");
            then.status(200)
                .header("content-type", "application/json")
                .body("{\"data\":[]}");
        });

        let provider = OpenAiProvider::new("test-key").with_base_url(&server.base_url());
        let valid = provider.validate_key().await.unwrap();
        assert!(valid);
        mock.assert();
    }

    #[tokio::test]
    async fn chat_parses_openai_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "model":"gpt-4o-mini",
                        "choices":[{"message":{"role":"assistant","content":"hello user"}}],
                        "usage":{"prompt_tokens":10,"completion_tokens":4,"total_tokens":14}
                    }"#,
                );
        });

        let provider = OpenAiProvider::new("test-key").with_base_url(&server.base_url());
        let response = provider
            .chat(&ChatRequest {
                model: "gpt-4o-mini".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: "say hello".to_string(),
                }],
                max_tokens: Some(64),
                temperature: Some(0.3),
                stream: false,
            })
            .await
            .unwrap();

        assert_eq!(response.content, "hello user");
        assert_eq!(response.model, "gpt-4o-mini");
        assert_eq!(response.usage.total_tokens, 14);
        mock.assert();
    }

    #[tokio::test]
    async fn embed_parses_vector_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/embeddings")
                .header("authorization", "Bearer test-key");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"data":[{"embedding":[0.1,0.2,0.3]}]}"#);
        });

        let provider = OpenAiProvider::new("test-key").with_base_url(&server.base_url());
        let embedding = provider.embed("hello").await.unwrap();
        assert_eq!(embedding, vec![0.1, 0.2, 0.3]);
        mock.assert();
    }

    #[tokio::test]
    async fn chat_returns_error_on_non_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(401).body("unauthorized");
        });

        let provider = OpenAiProvider::new("bad-key").with_base_url(&server.base_url());
        let result = provider
            .chat(&ChatRequest {
                model: "gpt-4o-mini".to_string(),
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
        let provider = OpenAiProvider::new("");
        let valid = provider.validate_key().await.unwrap();
        assert!(!valid);
    }

    #[tokio::test]
    async fn chat_rate_limit_429_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(429).body("rate limited");
        });
        let provider = OpenAiProvider::new("key").with_base_url(&server.base_url());
        let result = provider
            .chat(&ChatRequest {
                model: "gpt-4o".to_string(),
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
    async fn chat_malformed_response_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).body("not json at all");
        });
        let provider = OpenAiProvider::new("key").with_base_url(&server.base_url());
        let result = provider
            .chat(&ChatRequest {
                model: "gpt-4o".to_string(),
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
}
