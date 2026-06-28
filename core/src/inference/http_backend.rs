//! HTTP 推理后端
//!
//! 通过 OpenAI 兼容的 `/v1/completions` API 执行推理，
//! 适用于云端或自托管的 OpenAI 兼容服务。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::backend::{InferenceBackend, InferenceRequest, InferenceResponse};

/// HTTP 推理后端配置
#[derive(Debug, Clone)]
pub struct HttpInferenceConfig {
    /// API base URL (e.g. "https://api.openai.com")
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 模型名称 (e.g. "gpt-3.5-turbo-instruct")
    pub model_name: String,
}

/// HTTP 推理后端，实现 OpenAI 兼容的 completions API
pub struct HttpInferenceBackend {
    config: HttpInferenceConfig,
    client: Client,
}

impl HttpInferenceBackend {
    /// 创建新的 HTTP 推理后端
    pub fn new(config: HttpInferenceConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 创建带自定义 base URL 的后端（用于测试）
    pub fn with_base_url(api_key: &str, model_name: &str, base_url: &str) -> Self {
        Self {
            config: HttpInferenceConfig {
                base_url: base_url.to_string(),
                api_key: api_key.to_string(),
                model_name: model_name.to_string(),
            },
            client: Client::new(),
        }
    }
}

/// OpenAI completions 请求体
#[derive(Debug, Serialize)]
struct CompletionsRequest {
    model: String,
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    top_p: f32,
}

/// OpenAI completions 响应体
#[derive(Debug, Deserialize)]
struct CompletionsResponse {
    choices: Vec<CompletionChoice>,
    usage: Option<CompletionsUsage>,
}

#[derive(Debug, Deserialize)]
struct CompletionChoice {
    text: String,
}

#[derive(Debug, Deserialize)]
struct CompletionsUsage {
    total_tokens: usize,
}

#[async_trait]
impl InferenceBackend for HttpInferenceBackend {
    fn name(&self) -> &str {
        "http-openai-completions"
    }

    /// 云端后端无需本地加载模型，no-op
    async fn load_model(&mut self, _model_path: &std::path::Path) -> anyhow::Result<()> {
        Ok(())
    }

    /// 向 /v1/completions 发送推理请求
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
        let start = std::time::Instant::now();

        let body = CompletionsRequest {
            model: self.config.model_name.clone(),
            prompt: request.prompt.clone(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
        };

        let resp = self
            .client
            .post(format!("{}/v1/completions", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("completions API failed ({}): {}", status, text);
        }

        let api_resp: CompletionsResponse = resp.json().await?;
        let text = api_resp
            .choices
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();
        let tokens_used = api_resp
            .usage
            .map(|u| u.total_tokens)
            .unwrap_or(0);
        let latency_ms = start.elapsed().as_millis() as u64;

        Ok(InferenceResponse {
            text,
            tokens_used,
            latency_ms,
        })
    }

    /// 云端后端无需卸载，no-op
    async fn unload(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    /// 云端后端不占用本地内存
    fn memory_usage(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn mock_completions_response(text: &str, total_tokens: usize) -> serde_json::Value {
        serde_json::json!({
            "id": "cmpl-test",
            "object": "text_completion",
            "choices": [{"text": text, "index": 0, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 5, "completion_tokens": total_tokens - 5, "total_tokens": total_tokens}
        })
    }

    #[tokio::test]
    async fn infer_sends_correct_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/completions")
                .header("Authorization", "Bearer test-key")
                .body_contains(r#""model":"test-model""#);
            then.status(200)
                .json_body(mock_completions_response("hello world", 12));
        });

        let backend =
            HttpInferenceBackend::with_base_url("test-key", "test-model", &server.base_url());
        let req = InferenceRequest {
            prompt: "Say hello".into(),
            max_tokens: 50,
            temperature: 0.7,
            top_p: 0.9,
        };
        let resp = backend.infer(&req).await.unwrap();

        assert_eq!(resp.text, "hello world");
        assert_eq!(resp.tokens_used, 12);
        mock.assert();
    }

    #[tokio::test]
    async fn infer_includes_bearer_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/completions")
                .header("Authorization", "Bearer sk-secret-123");
            then.status(200)
                .json_body(mock_completions_response("ok", 8));
        });

        let backend =
            HttpInferenceBackend::with_base_url("sk-secret-123", "gpt-3.5", &server.base_url());
        let req = InferenceRequest {
            prompt: "test".into(),
            max_tokens: 10,
            temperature: 0.5,
            top_p: 1.0,
        };
        backend.infer(&req).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn infer_handles_api_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/completions");
            then.status(429).body("rate limited");
        });

        let backend =
            HttpInferenceBackend::with_base_url("key", "model", &server.base_url());
        let req = InferenceRequest {
            prompt: "test".into(),
            max_tokens: 10,
            temperature: 0.5,
            top_p: 1.0,
        };
        let result = backend.infer(&req).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("429"));
    }

    #[tokio::test]
    async fn load_model_is_noop() {
        let mut backend =
            HttpInferenceBackend::with_base_url("key", "model", "http://localhost:9999");
        let result = backend.load_model(std::path::Path::new("/tmp/model")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn unload_is_noop() {
        let mut backend =
            HttpInferenceBackend::with_base_url("key", "model", "http://localhost:9999");
        let result = backend.unload().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn memory_usage_returns_zero() {
        let backend =
            HttpInferenceBackend::with_base_url("key", "model", "http://localhost:9999");
        assert_eq!(backend.memory_usage(), 0);
        assert_eq!(backend.name(), "http-openai-completions");
    }
}
