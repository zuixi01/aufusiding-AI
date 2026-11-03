//! 云端模型连接池 — 管理 LLM Provider 实例的获取和释放

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

/// 模型偏好 — Agent 请求模型时的偏好
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModelTier {
    /// 最强推理能力（Opus/GPT-4o）
    Premium,
    /// 平衡性能和成本（Sonnet/GPT-4o-mini）
    Standard,
    /// 最快速度（Haiku/GPT-3.5）
    Fast,
}

/// 模型句柄 — 从池中获取的模型连接
#[derive(Debug, Clone)]
pub struct ModelHandle {
    pub id: String,
    pub provider_name: String,
    pub model_name: String,
    pub tier: ModelTier,
}

/// 模型后端 trait — 抽象 LLM 调用
#[async_trait]
pub trait ModelBackend: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<String>;
}

/// 简化的聊天消息（用于模型调用）
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

type BackendPool = HashMap<ModelTier, Vec<Arc<dyn ModelBackend>>>;

/// 模型连接池
pub struct ModelPool {
    backends: Arc<RwLock<BackendPool>>,
}

impl ModelPool {
    pub fn new() -> Self {
        Self {
            backends: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册模型后端
    pub async fn register(&self, tier: ModelTier, backend: Arc<dyn ModelBackend>) {
        let mut backends = self.backends.write().await;
        backends.entry(tier).or_default().push(backend);
    }

    /// 获取指定层级的模型（轮询选择）
    pub async fn acquire(&self, tier: &ModelTier) -> anyhow::Result<ModelHandle> {
        let backends = self.backends.read().await;
        let pool = backends
            .get(tier)
            .ok_or_else(|| anyhow::anyhow!("no model available for tier {:?}", tier))?;
        let backend = pool
            .first()
            .ok_or_else(|| anyhow::anyhow!("model pool empty for tier {:?}", tier))?;
        Ok(ModelHandle {
            id: uuid::Uuid::new_v4().to_string(),
            provider_name: backend.provider_name().to_string(),
            model_name: backend.model_name().to_string(),
            tier: tier.clone(),
        })
    }

    /// 获取后端引用用于实际调用
    pub async fn get_backend(&self, tier: &ModelTier) -> Option<Arc<dyn ModelBackend>> {
        let backends = self.backends.read().await;
        backends.get(tier).and_then(|pool| pool.first().cloned())
    }

    /// 已注册的模型数量
    pub async fn total_models(&self) -> usize {
        let backends = self.backends.read().await;
        backends.values().map(|v| v.len()).sum()
    }

    /// 检查指定层级是否有可用模型
    pub async fn has_tier(&self, tier: &ModelTier) -> bool {
        let backends = self.backends.read().await;
        backends.get(tier).map(|v| !v.is_empty()).unwrap_or(false)
    }
}

/// Mock 模型后端（用于测试）
pub struct MockModelBackend {
    provider: String,
    model: String,
    response: String,
}

impl MockModelBackend {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        response: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            response: response.into(),
        }
    }
}

#[async_trait]
impl ModelBackend for MockModelBackend {
    fn provider_name(&self) -> &str {
        &self.provider
    }
    fn model_name(&self) -> &str {
        &self.model
    }
    async fn chat(&self, _messages: &[ChatMessage]) -> anyhow::Result<String> {
        Ok(self.response.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_backend(tier_label: &str) -> Arc<dyn ModelBackend> {
        Arc::new(MockModelBackend::new(
            "mock",
            format!("mock-{}", tier_label),
            "test response",
        ))
    }

    #[tokio::test]
    async fn register_and_acquire() {
        let pool = ModelPool::new();
        pool.register(ModelTier::Standard, mock_backend("std"))
            .await;
        let handle = pool.acquire(&ModelTier::Standard).await.unwrap();
        assert_eq!(handle.provider_name, "mock");
        assert_eq!(handle.tier, ModelTier::Standard);
    }

    #[tokio::test]
    async fn acquire_empty_tier_fails() {
        let pool = ModelPool::new();
        let result = pool.acquire(&ModelTier::Premium).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn has_tier_check() {
        let pool = ModelPool::new();
        assert!(!pool.has_tier(&ModelTier::Fast).await);
        pool.register(ModelTier::Fast, mock_backend("fast")).await;
        assert!(pool.has_tier(&ModelTier::Fast).await);
    }

    #[tokio::test]
    async fn total_models_count() {
        let pool = ModelPool::new();
        pool.register(ModelTier::Standard, mock_backend("s1")).await;
        pool.register(ModelTier::Standard, mock_backend("s2")).await;
        pool.register(ModelTier::Premium, mock_backend("p1")).await;
        assert_eq!(pool.total_models().await, 3);
    }

    #[tokio::test]
    async fn get_backend_and_chat() {
        let pool = ModelPool::new();
        pool.register(
            ModelTier::Standard,
            Arc::new(MockModelBackend::new("test", "gpt-4", "hello world")),
        )
        .await;
        let backend = pool.get_backend(&ModelTier::Standard).await.unwrap();
        let response = backend.chat(&[]).await.unwrap();
        assert_eq!(response, "hello world");
    }

    #[tokio::test]
    async fn get_backend_missing_tier() {
        let pool = ModelPool::new();
        assert!(pool.get_backend(&ModelTier::Premium).await.is_none());
    }
}
