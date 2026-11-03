//! 推理后端抽象
//!
//! 定义统一的推理接口，所有本地推理引擎都需实现此 trait。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 推理请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// 提示词
    pub prompt: String,
    /// 最大生成 token 数
    pub max_tokens: usize,
    /// 温度参数
    pub temperature: f32,
    /// Top-P 采样
    pub top_p: f32,
}

/// 推理响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    /// 生成的文本
    pub text: String,
    /// 使用的 token 数
    pub tokens_used: usize,
    /// 推理耗时（毫秒）
    pub latency_ms: u64,
}

/// 推理后端 trait
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// 后端名称
    fn name(&self) -> &str;

    /// 加载模型
    async fn load_model(&mut self, model_path: &std::path::Path) -> anyhow::Result<()>;

    /// 执行推理
    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse>;

    /// 卸载模型，释放资源
    async fn unload(&mut self) -> anyhow::Result<()>;

    /// 查询当前显存/内存占用（字节）
    fn memory_usage(&self) -> usize;
}

/// 测试用 Mock 推理后端
pub struct MockInferenceBackend {
    backend_name: String,
    loaded: std::sync::Mutex<bool>,
    response_text: String,
}

impl MockInferenceBackend {
    pub fn new() -> Self {
        Self {
            backend_name: "mock".to_string(),
            loaded: std::sync::Mutex::new(false),
            response_text: "mock response".to_string(),
        }
    }

    pub fn with_response(mut self, text: &str) -> Self {
        self.response_text = text.to_string();
        self
    }

    pub fn is_loaded(&self) -> bool {
        *self.loaded.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl InferenceBackend for MockInferenceBackend {
    fn name(&self) -> &str {
        &self.backend_name
    }

    async fn load_model(&mut self, _model_path: &std::path::Path) -> anyhow::Result<()> {
        *self.loaded.lock().unwrap_or_else(|e| e.into_inner()) = true;
        Ok(())
    }

    async fn infer(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse> {
        Ok(InferenceResponse {
            text: self.response_text.clone(),
            tokens_used: request.max_tokens.min(10),
            latency_ms: 1,
        })
    }

    async fn unload(&mut self) -> anyhow::Result<()> {
        *self.loaded.lock().unwrap_or_else(|e| e.into_inner()) = false;
        Ok(())
    }

    fn memory_usage(&self) -> usize {
        if *self.loaded.lock().unwrap_or_else(|e| e.into_inner()) {
            1024
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_backend_load_and_infer() {
        let mut backend = MockInferenceBackend::new();
        assert!(!backend.is_loaded());
        backend
            .load_model(std::path::Path::new("/tmp/model"))
            .await
            .unwrap();
        assert!(backend.is_loaded());
        let req = InferenceRequest {
            prompt: "hello".into(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 0.9,
        };
        let resp = backend.infer(&req).await.unwrap();
        assert_eq!(resp.text, "mock response");
    }

    #[tokio::test]
    async fn mock_backend_unload() {
        let mut backend = MockInferenceBackend::new();
        backend
            .load_model(std::path::Path::new("/tmp/m"))
            .await
            .unwrap();
        assert_eq!(backend.memory_usage(), 1024);
        backend.unload().await.unwrap();
        assert_eq!(backend.memory_usage(), 0);
    }

    #[tokio::test]
    async fn mock_backend_custom_response() {
        let backend = MockInferenceBackend::new().with_response("custom");
        let req = InferenceRequest {
            prompt: "test".into(),
            max_tokens: 50,
            temperature: 0.5,
            top_p: 1.0,
        };
        let resp = backend.infer(&req).await.unwrap();
        assert_eq!(resp.text, "custom");
    }

    #[test]
    fn mock_backend_name() {
        let backend = MockInferenceBackend::new();
        assert_eq!(backend.name(), "mock");
    }
}
