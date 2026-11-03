//! llama.cpp 绑定
//!
//! 通过 HTTP 调用 llama-server（OpenAI 兼容 API），支持 GGUF 格式模型的本地推理。
//! 适用于所有平台（macOS/Linux/Windows）。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// OpenAI-compatible chat completion request
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: usize,
    temperature: f32,
    top_p: f32,
}

/// Chat message
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// OpenAI-compatible chat completion response
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    usage: Option<UsageInfo>,
}

/// A single choice in the response
#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChoiceMessage,
}

/// Message within a choice
#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: String,
}

/// Token usage info
#[derive(Debug, Deserialize)]
struct UsageInfo {
    total_tokens: Option<usize>,
}

/// llama.cpp 推理后端
pub struct LlamaBackend {
    /// 模型路径
    model_path: Option<PathBuf>,
    /// 上下文大小
    context_size: usize,
    /// GPU 层数（0 = 纯 CPU）
    gpu_layers: u32,
    /// 估算内存占用
    memory_bytes: usize,
    /// llama-server URL
    server_url: String,
    /// HTTP client
    client: reqwest::Client,
}

impl LlamaBackend {
    /// 创建 llama.cpp 后端
    pub fn new(context_size: usize, gpu_layers: u32) -> Self {
        Self {
            model_path: None,
            context_size,
            gpu_layers,
            memory_bytes: 0,
            server_url: "http://localhost:8080".to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// 创建指定服务器 URL 的 llama.cpp 后端
    pub fn new_with_url(context_size: usize, gpu_layers: u32, server_url: String) -> Self {
        Self {
            model_path: None,
            context_size,
            gpu_layers,
            memory_bytes: 0,
            server_url,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl super::backend::InferenceBackend for LlamaBackend {
    fn name(&self) -> &str {
        "llama.cpp"
    }

    async fn load_model(&mut self, model_path: &Path) -> anyhow::Result<()> {
        if !model_path.exists() {
            anyhow::bail!("model file not found: {}", model_path.display());
        }

        self.model_path = Some(model_path.to_path_buf());
        let base = 512 * 1024 * 1024usize;
        let ctx = self.context_size.saturating_mul(2048);
        let gpu = self.gpu_layers as usize * 4 * 1024 * 1024;
        self.memory_bytes = base.saturating_add(ctx).saturating_add(gpu);
        Ok(())
    }

    async fn infer(
        &self,
        request: &super::backend::InferenceRequest,
    ) -> anyhow::Result<super::backend::InferenceResponse> {
        if self.model_path.is_none() {
            anyhow::bail!("llama backend model not loaded");
        }

        let start = Instant::now();
        let prompt = request.prompt.trim();
        let url = format!("{}/v1/chat/completions", self.server_url);

        let body = ChatCompletionRequest {
            model: "default".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "llama-server not available at {}: {}. Ensure llama-server is running.",
                    self.server_url,
                    e
                )
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            anyhow::bail!("llama-server returned error {}: {}", status, body_text);
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("failed to parse llama-server response: {}", e))?;

        let text = completion
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let tokens_used = completion
            .usage
            .and_then(|u| u.total_tokens)
            .unwrap_or_else(|| prompt.split_whitespace().count() + request.max_tokens);

        Ok(super::backend::InferenceResponse {
            text,
            tokens_used,
            latency_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn unload(&mut self) -> anyhow::Result<()> {
        self.model_path = None;
        self.memory_bytes = 0;
        Ok(())
    }

    fn memory_usage(&self) -> usize {
        self.memory_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::backend::InferenceBackend;

    #[tokio::test]
    async fn infer_requires_loaded_model() {
        let backend = LlamaBackend::new(2048, 0);
        let result = backend
            .infer(&super::super::backend::InferenceRequest {
                prompt: "hello".to_string(),
                max_tokens: 32,
                temperature: 0.7,
                top_p: 0.9,
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn load_infer_unload_flow() {
        let model_path = crate::test_support::temp_file_path("llama-test", "gguf");
        let _ = std::fs::write(&model_path, b"gguf");

        let mut backend = LlamaBackend::new(2048, 10);
        backend.load_model(&model_path).await.unwrap();
        assert!(backend.memory_usage() > 0);

        // Real inference requires a running llama-server; verify error is clear
        let result = backend
            .infer(&super::super::backend::InferenceRequest {
                prompt: "hello from prompt".to_string(),
                max_tokens: 32,
                temperature: 0.7,
                top_p: 0.9,
            })
            .await;

        // Server likely not running in test env - either we get a
        // connection error or a valid response; both are acceptable
        match result {
            Ok(response) => assert!(!response.text.is_empty()),
            Err(e) => assert!(e.to_string().contains("llama-server")),
        }

        backend.unload().await.unwrap();
        assert_eq!(backend.memory_usage(), 0);
    }

    #[tokio::test]
    async fn load_nonexistent_model_errors() {
        let mut backend = LlamaBackend::new(2048, 0);
        let result = backend
            .load_model(std::path::Path::new("/nonexistent/model.gguf"))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn unload_clears_memory() {
        let model_path = crate::test_support::temp_file_path("llama-unload", "gguf");
        let _ = std::fs::write(&model_path, b"gguf");

        let mut backend = LlamaBackend::new(2048, 0);
        backend.load_model(&model_path).await.unwrap();
        assert!(backend.memory_usage() > 0);

        backend.unload().await.unwrap();
        assert_eq!(backend.memory_usage(), 0);
        assert!(backend.model_path.is_none());
    }

    #[tokio::test]
    async fn memory_usage_increases_after_load() {
        let model_path = crate::test_support::temp_file_path("llama-mem", "gguf");
        let _ = std::fs::write(&model_path, b"gguf");

        let mut backend = LlamaBackend::new(2048, 5);
        assert_eq!(backend.memory_usage(), 0);

        backend.load_model(&model_path).await.unwrap();
        assert!(backend.memory_usage() > 0);
    }

    #[tokio::test]
    async fn double_load_replaces_model() {
        let path_a = crate::test_support::temp_file_path("llama-a", "gguf");
        let path_b = crate::test_support::temp_file_path("llama-b", "gguf");
        let _ = std::fs::write(&path_a, b"model-a");
        let _ = std::fs::write(&path_b, b"model-b");

        let mut backend = LlamaBackend::new(2048, 0);
        backend.load_model(&path_a).await.unwrap();
        let mem_a = backend.memory_usage();

        backend.load_model(&path_b).await.unwrap();
        assert_eq!(backend.model_path.as_ref().unwrap(), &path_b);
        assert_eq!(backend.memory_usage(), mem_a);
    }

    #[tokio::test]
    async fn new_with_url_sets_server_url() {
        let backend = LlamaBackend::new_with_url(2048, 0, "http://myhost:9090".to_string());
        assert_eq!(backend.server_url, "http://myhost:9090");
    }

    #[tokio::test]
    async fn infer_with_mock_server() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    serde_json::json!({
                        "choices": [{
                            "message": { "role": "assistant", "content": "Hello back!" }
                        }],
                        "usage": { "total_tokens": 10 }
                    })
                    .to_string(),
                );
        });

        let model_path = crate::test_support::temp_file_path("llama-mock-srv", "gguf");
        let _ = std::fs::write(&model_path, b"gguf");

        let mut backend = LlamaBackend::new_with_url(2048, 0, server.url(""));
        backend.load_model(&model_path).await.unwrap();

        let response = backend
            .infer(&super::super::backend::InferenceRequest {
                prompt: "hello".to_string(),
                max_tokens: 32,
                temperature: 0.7,
                top_p: 0.9,
            })
            .await
            .unwrap();

        assert_eq!(response.text, "Hello back!");
        assert_eq!(response.tokens_used, 10);
        mock.assert();
    }
}
