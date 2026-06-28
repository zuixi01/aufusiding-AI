//! Anthropic API 适配器
//!
//! 支持 Claude 3.5 Sonnet、Claude 3 Opus 等 Anthropic 模型。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Anthropic Provider
pub struct AnthropicProvider {
    /// API 密钥
    api_key: String,
    /// API 版本
    api_version: String,
    /// API 端点
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// 创建 Anthropic Provider
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            api_version: "2023-06-01".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// 使用自定义端点（测试或代理场景）
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

#[async_trait]
impl super::traits::Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn available_models(&self) -> Vec<String> {
        vec![
            "claude-3-5-sonnet-20241022".to_string(),
            "claude-3-opus-20240229".to_string(),
            "claude-3-haiku-20240307".to_string(),
        ]
    }

    async fn chat(
        &self,
        request: &super::traits::ChatRequest,
    ) -> anyhow::Result<super::traits::ChatResponse> {
        let (system, messages) = split_system_and_messages(&request.messages);
        let payload = AnthropicChatRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(1024),
            temperature: request.temperature,
            stream: request.stream,
            system,
            messages,
        };

        let response = self
            .client
            .post(self.endpoint("messages"))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|e| {
                tracing::warn!("failed to read anthropic chat error body: {}", e);
                String::new()
            });
            anyhow::bail!("anthropic chat failed: status={}, body={}", status, body);
        }

        let parsed: AnthropicChatResponse = response.json().await?;
        let content = parsed
            .content
            .iter()
            .find_map(|c| c.text.clone())
            .unwrap_or_default();

        Ok(super::traits::ChatResponse {
            content,
            model: parsed.model,
            usage: super::traits::TokenUsage {
                prompt_tokens: parsed.usage.input_tokens,
                completion_tokens: parsed.usage.output_tokens,
                total_tokens: parsed.usage.input_tokens + parsed.usage.output_tokens,
            },
        })
    }

    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("Anthropic 暂不支持 Embeddings API")
    }

    async fn validate_key(&self) -> anyhow::Result<bool> {
        if self.api_key.trim().is_empty() {
            return Ok(false);
        }

        let response = self
            .client
            .get(self.endpoint("models"))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("anthropic key validation request failed: {}", e))?;

        let status = response.status().as_u16();
        match status {
            200 => Ok(true),
            401 => Ok(false),
            _ => {
                let body = response.text().await.unwrap_or_else(|e| {
                    tracing::warn!("failed to read anthropic validation error body: {}", e);
                    String::new()
                });
                anyhow::bail!(
                    "anthropic key validation unexpected status={}, body={}",
                    status,
                    body
                )
            }
        }
    }
}

fn split_system_and_messages(
    source: &[super::traits::ChatMessage],
) -> (Option<String>, Vec<AnthropicMessage>) {
    let mut system: Option<String> = None;
    let mut messages = Vec::new();

    for msg in source {
        if msg.role == "system" {
            if system.is_none() {
                system = Some(msg.content.clone());
            }
            continue;
        }

        let role = if msg.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        messages.push(AnthropicMessage {
            role: role.to_string(),
            content: msg.content.clone(),
        });
    }

    (system, messages)
}

#[derive(Debug, Serialize)]
struct AnthropicChatRequest {
    model: String,
    max_tokens: usize,
    temperature: Option<f32>,
    stream: bool,
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicChatResponse {
    model: String,
    content: Vec<AnthropicContentBlock>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[allow(dead_code)]
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: usize,
    output_tokens: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::traits::{ChatMessage, ChatRequest, Provider};
    use httpmock::prelude::*;

    #[tokio::test]
    async fn validate_key_checks_messages_endpoint() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/models")
                .header("x-api-key", "test-key")
                .header("anthropic-version", "2023-06-01");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"data":[]}"#);
        });

        let provider = AnthropicProvider::new("test-key").with_base_url(&server.base_url());
        let valid = provider.validate_key().await.unwrap();
        assert!(valid);
        mock.assert();
    }

    #[tokio::test]
    async fn chat_parses_anthropic_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/messages")
                .header("x-api-key", "test-key");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "model":"claude-3-5-sonnet-20241022",
                        "content":[
                            {"type":"text","text":"hello from claude"}
                        ],
                        "usage":{"input_tokens":11,"output_tokens":5}
                    }"#,
                );
        });

        let provider = AnthropicProvider::new("test-key").with_base_url(&server.base_url());
        let response = provider
            .chat(&ChatRequest {
                model: "claude-3-5-sonnet-20241022".to_string(),
                messages: vec![
                    ChatMessage {
                        role: "system".to_string(),
                        content: "be concise".to_string(),
                    },
                    ChatMessage {
                        role: "user".to_string(),
                        content: "say hello".to_string(),
                    },
                ],
                max_tokens: Some(64),
                temperature: Some(0.2),
                stream: false,
            })
            .await
            .unwrap();

        assert_eq!(response.content, "hello from claude");
        assert_eq!(response.model, "claude-3-5-sonnet-20241022");
        assert_eq!(response.usage.total_tokens, 16);
        mock.assert();
    }

    #[tokio::test]
    async fn chat_returns_error_on_non_success() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/messages");
            then.status(401).body("unauthorized");
        });

        let provider = AnthropicProvider::new("bad-key").with_base_url(&server.base_url());
        let result = provider
            .chat(&ChatRequest {
                model: "claude-3-5-sonnet-20241022".to_string(),
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
    async fn embed_not_supported_returns_error() {
        let provider = AnthropicProvider::new("key");
        let result = provider.embed("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("不支持"));
    }

    #[tokio::test]
    async fn validate_key_empty_returns_false() {
        let provider = AnthropicProvider::new("");
        let valid = provider.validate_key().await.unwrap();
        assert!(!valid);
    }

    #[tokio::test]
    async fn chat_rate_limit_429_returns_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/messages");
            then.status(429).body("rate limited");
        });
        let provider = AnthropicProvider::new("key").with_base_url(&server.base_url());
        let result = provider
            .chat(&ChatRequest {
                model: "claude-3-5-sonnet-20241022".to_string(),
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
}
