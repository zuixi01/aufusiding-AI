//! Embedding 服务抽象层
//!
//! 统一的向量嵌入接口，支持注入不同后端（Mock、Ollama、OpenAI 等）。

use async_trait::async_trait;

/// Embedding 服务 trait
#[async_trait]
pub trait EmbeddingService: Send + Sync {
    /// 生成文本的向量嵌入
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;

    /// 返回向量维度
    fn dimension(&self) -> usize;
}

/// Mock Embedding 服务（使用 sin 函数生成伪向量，用于测试）
pub struct MockEmbeddingService {
    dimension: usize,
}

impl MockEmbeddingService {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait]
impl EmbeddingService for MockEmbeddingService {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut embedding = vec![0.0f32; self.dimension];

        // 用字节内容生成哈希，确保不同文本产生不同向量
        let mut hash: u64 = 5381;
        for byte in text.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }

        for (i, val) in embedding.iter_mut().enumerate() {
            let seed = hash.wrapping_add(i as u64);
            *val = ((seed as f32) * 0.1).sin();
        }

        // 归一化
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Provider Embedding 服务（包装 Provider.embed()）
pub struct ProviderEmbeddingService {
    provider: std::sync::Arc<dyn crate::providers::traits::Provider>,
    dimension: usize,
}

impl ProviderEmbeddingService {
    pub fn new(
        provider: std::sync::Arc<dyn crate::providers::traits::Provider>,
        dimension: usize,
    ) -> Self {
        Self {
            provider,
            dimension,
        }
    }

    /// 自动检测维度（通过一次 embed 调用）
    pub async fn detect_dimension(
        provider: std::sync::Arc<dyn crate::providers::traits::Provider>,
    ) -> anyhow::Result<Self> {
        let sample = provider.embed("dimension probe").await?;
        let dimension = sample.len();
        Ok(Self {
            provider,
            dimension,
        })
    }
}

#[async_trait]
impl EmbeddingService for ProviderEmbeddingService {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        self.provider.embed(text).await
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Ollama Embedding 服务（直接调用 Ollama embed API）
pub struct OllamaEmbeddingService {
    client: reqwest::Client,
    base_url: String,
    model: String,
    dimension: usize,
}

impl Default for OllamaEmbeddingService {
    fn default() -> Self {
        Self::new("http://localhost:11434", "all-minilm", 384)
    }
}

impl OllamaEmbeddingService {
    pub fn new(base_url: &str, model: &str, dimension: usize) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            dimension,
        }
    }
}

#[derive(serde::Serialize)]
struct OllamaEmbedPayload {
    model: String,
    input: String,
}

#[derive(serde::Deserialize)]
struct OllamaEmbedResult {
    embeddings: Vec<Vec<f32>>,
}

#[async_trait]
impl EmbeddingService for OllamaEmbeddingService {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let url = format!("{}/api/embed", self.base_url);
        let payload = OllamaEmbedPayload {
            model: self.model.clone(),
            input: text.to_string(),
        };

        let resp = self.client.post(&url).json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama embed failed ({}): {}", status, body);
        }

        let parsed: OllamaEmbedResult = resp.json().await?;
        parsed
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Ollama returned empty embeddings"))
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_embed_deterministic() {
        let service = MockEmbeddingService::new(128);
        let v1 = service.embed("hello").await.unwrap();
        let v2 = service.embed("hello").await.unwrap();
        assert_eq!(v1, v2);
    }

    #[tokio::test]
    async fn test_mock_embed_dimension() {
        let service = MockEmbeddingService::new(384);
        let v = service.embed("test").await.unwrap();
        assert_eq!(v.len(), 384);
        assert_eq!(service.dimension(), 384);
    }

    #[tokio::test]
    async fn test_mock_embed_normalized() {
        let service = MockEmbeddingService::new(128);
        let v = service.embed("normalize test").await.unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_mock_different_inputs_different_vectors() {
        let service = MockEmbeddingService::new(128);
        let v1 = service.embed("hello").await.unwrap();
        let v2 = service.embed("world").await.unwrap();
        assert_ne!(v1, v2);
    }

    #[tokio::test]
    async fn test_provider_embedding_service() {
        use crate::providers::mock::MockProvider;
        use std::sync::Arc;

        let provider = Arc::new(MockProvider::new("test"));
        let service = ProviderEmbeddingService::new(provider, 3);

        let v = service.embed("hello").await.unwrap();
        assert!(!v.is_empty());
        assert_eq!(service.dimension(), 3);
    }

    #[tokio::test]
    async fn test_provider_detect_dimension() {
        use crate::providers::mock::MockProvider;
        use std::sync::Arc;

        let provider = Arc::new(MockProvider::new("test"));
        let service = ProviderEmbeddingService::detect_dimension(provider)
            .await
            .unwrap();

        // MockProvider 返回 3 维向量
        assert_eq!(service.dimension(), 3);
    }

    #[tokio::test]
    async fn test_trait_object_usage() {
        // 验证 trait 可以作为 dyn 使用
        let service: Box<dyn EmbeddingService> = Box::new(MockEmbeddingService::new(64));
        let v = service.embed("trait object").await.unwrap();
        assert_eq!(v.len(), 64);
    }

    #[tokio::test]
    async fn test_ollama_embed_service_success() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/embed");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"embeddings":[[0.1,0.2,0.3]]}"#);
        });

        let service = OllamaEmbeddingService::new(&server.base_url(), "all-minilm", 3);
        let v = service.embed("hello").await.unwrap();
        assert_eq!(v.len(), 3);
        assert!((v[0] - 0.1).abs() < 0.001);
        assert_eq!(service.dimension(), 3);
    }

    #[tokio::test]
    async fn test_ollama_embed_service_error() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/embed");
            then.status(500).body("internal error");
        });

        let service = OllamaEmbeddingService::new(&server.base_url(), "all-minilm", 3);
        let result = service.embed("hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ollama_embed_service_empty_response() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/embed");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"embeddings":[]}"#);
        });

        let service = OllamaEmbeddingService::new(&server.base_url(), "all-minilm", 3);
        let result = service.embed("hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ollama_embed_service_default() {
        let service = OllamaEmbeddingService::default();
        assert_eq!(service.base_url, "http://localhost:11434");
        assert_eq!(service.model, "all-minilm");
        assert_eq!(service.dimension(), 384);
    }

    #[tokio::test]
    async fn test_ollama_embed_service_as_trait_object() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/embed");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"embeddings":[[1.0,2.0]]}"#);
        });

        let service: Box<dyn EmbeddingService> =
            Box::new(OllamaEmbeddingService::new(&server.base_url(), "test", 2));
        let v = service.embed("test").await.unwrap();
        assert_eq!(v.len(), 2);
    }
}
