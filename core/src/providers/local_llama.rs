//! 本地 llama Provider
//!
//! 将 `LlamaBackend` 适配为统一 `Provider` 接口。

use super::traits::{ChatRequest, ChatResponse, Provider, TokenUsage};
use crate::inference::backend::{InferenceBackend, InferenceRequest};
use crate::inference::llama::LlamaBackend;
use async_trait::async_trait;
use std::path::Path;
use tokio::sync::RwLock;

pub struct LocalLlamaProvider {
    name: String,
    backend: RwLock<LlamaBackend>,
}

impl LocalLlamaProvider {
    pub fn new(name: &str, context_size: usize, gpu_layers: u32) -> Self {
        Self {
            name: name.to_string(),
            backend: RwLock::new(LlamaBackend::new(context_size, gpu_layers)),
        }
    }

    pub async fn load_model(&self, model_path: &Path) -> anyhow::Result<()> {
        let mut backend = self.backend.write().await;
        backend.load_model(model_path).await
    }

    pub async fn unload_model(&self) -> anyhow::Result<()> {
        let mut backend = self.backend.write().await;
        backend.unload().await
    }
}

#[async_trait]
impl Provider for LocalLlamaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn available_models(&self) -> Vec<String> {
        vec!["local-llama".to_string()]
    }

    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse> {
        let prompt = request
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let inference_request = InferenceRequest {
            prompt,
            max_tokens: request.max_tokens.unwrap_or(256),
            temperature: request.temperature.unwrap_or(0.7),
            top_p: 0.9,
        };

        let backend = self.backend.read().await;
        let result = backend.infer(&inference_request).await?;

        let prompt_tokens = request.messages.len().max(1) * 8;
        let completion_tokens = result.tokens_used.saturating_sub(prompt_tokens).max(1);

        Ok(ChatResponse {
            content: result.text,
            model: request.model.clone(),
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        })
    }

    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("local llama backend does not support embeddings; use an embedding-specific model or API")
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
    async fn local_provider_requires_loaded_model() {
        let provider = LocalLlamaProvider::new("local-llama", 2048, 0);
        let result = provider
            .chat(&ChatRequest {
                model: "local-llama".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: "hello".to_string(),
                }],
                max_tokens: Some(32),
                temperature: Some(0.7),
                stream: false,
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn local_provider_chat_after_model_loaded() {
        let provider = LocalLlamaProvider::new("local-llama", 2048, 8);
        let model_path = crate::test_support::temp_file_path("local-llama-provider", "gguf");
        let _ = std::fs::write(&model_path, b"gguf");
        provider.load_model(&model_path).await.unwrap();

        let result = provider
            .chat(&ChatRequest {
                model: "local-llama".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: "hello local model".to_string(),
                }],
                max_tokens: Some(32),
                temperature: Some(0.7),
                stream: false,
            })
            .await;

        // With real llama-server: returns content; without: returns connection error
        match result {
            Ok(response) => {
                assert!(!response.content.is_empty());
                assert!(response.usage.total_tokens > 0);
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("llama-server") || msg.contains("error sending request"),
                    "unexpected error: {msg}"
                );
            }
        }

        provider.unload_model().await.unwrap();
    }
}
