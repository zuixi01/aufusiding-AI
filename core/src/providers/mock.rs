//! Mock Provider
//!
//! 用于本地测试与无网络开发场景。

use super::traits::{ChatRequest, ChatResponse, Provider, TokenUsage};
use async_trait::async_trait;

pub struct MockProvider {
    name: String,
}

impl MockProvider {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn available_models(&self) -> Vec<String> {
        vec!["mock-chat-1".to_string()]
    }

    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse> {
        let last_content = request
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let prompt_tokens = request
            .messages
            .iter()
            .map(|m| m.content.split_whitespace().count().max(1))
            .sum::<usize>();

        let completion_tokens = 8;

        Ok(ChatResponse {
            content: format!("[mock:{}] {}", self.name, last_content),
            model: request.model.clone(),
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        })
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let len = text.chars().count() as f32;
        Ok(vec![len, len / 2.0, 1.0])
    }

    async fn validate_key(&self) -> anyhow::Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::traits::ChatMessage;

    #[tokio::test]
    async fn mock_chat_returns_deterministic_reply() {
        let provider = MockProvider::new("mock-test");
        let response = provider
            .chat(&ChatRequest {
                model: "mock-chat-1".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: "hello world".to_string(),
                }],
                max_tokens: None,
                temperature: None,
                stream: false,
            })
            .await
            .unwrap();

        assert!(response.content.contains("hello world"));
        assert_eq!(response.model, "mock-chat-1");
        assert!(response.usage.total_tokens >= response.usage.prompt_tokens);
    }
}
