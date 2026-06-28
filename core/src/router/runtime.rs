//! 路由运行时
//!
//! 将路由策略输出与 Provider 注册表、Fallback 熔断机制连接起来。

use super::strategy::{RouteDecision, RouteTarget};
use crate::providers::registry::ProviderRegistry;
use crate::providers::traits::{ChatRequest, ChatResponse};
use crate::router::fallback::FallbackRouter;
use std::time::Instant;

/// 路由执行结果
#[derive(Debug, Clone)]
pub struct RoutedResponse {
    pub provider: String,
    pub response: ChatResponse,
    pub attempts: Vec<RouteAttempt>,
}

/// 单次 provider 尝试记录
#[derive(Debug, Clone)]
pub struct RouteAttempt {
    pub provider: String,
    pub success: bool,
    pub error: Option<String>,
}

/// 路由运行时
pub struct RoutingRuntime {
    registry: ProviderRegistry,
    fallback: FallbackRouter,
}

impl RoutingRuntime {
    pub fn new(registry: ProviderRegistry, fallback: FallbackRouter) -> Self {
        Self { registry, fallback }
    }

    pub async fn execute(
        &mut self,
        decision: &RouteDecision,
        request: &ChatRequest,
    ) -> anyhow::Result<RoutedResponse> {
        let preferred = preferred_provider_name(decision);
        let now = Instant::now();
        let candidates = self.fallback.available_candidates(&preferred, now);

        if candidates.is_empty() {
            anyhow::bail!("no available providers for preferred target: {preferred}");
        }

        let mut last_error: Option<anyhow::Error> = None;
        let mut attempts = Vec::new();

        for candidate in candidates {
            let Some(provider) = self.registry.get(&candidate) else {
                self.fallback.record_failure(&candidate, Instant::now());
                attempts.push(RouteAttempt {
                    provider: candidate,
                    success: false,
                    error: Some("provider not registered".to_string()),
                });
                continue;
            };

            match provider.chat(request).await {
                Ok(response) => {
                    self.fallback.record_success(&candidate);
                    attempts.push(RouteAttempt {
                        provider: candidate.clone(),
                        success: true,
                        error: None,
                    });
                    return Ok(RoutedResponse {
                        provider: candidate,
                        response,
                        attempts,
                    });
                }
                Err(err) => {
                    self.fallback.record_failure(&candidate, Instant::now());
                    attempts.push(RouteAttempt {
                        provider: candidate.clone(),
                        success: false,
                        error: Some(err.to_string()),
                    });
                    last_error = Some(err.context(format!("provider failed: {candidate}")));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("all providers unavailable")))
    }
}

fn preferred_provider_name(decision: &RouteDecision) -> String {
    match &decision.target {
        RouteTarget::Cloud(name) => normalize_provider_name(name),
        RouteTarget::Local(name) => normalize_provider_name(name),
        RouteTarget::Hybrid { cloud, .. } => normalize_provider_name(cloud),
    }
}

fn normalize_provider_name(target: &str) -> String {
    if target.starts_with("openai") {
        "openai".to_string()
    } else if target.starts_with("anthropic") {
        "anthropic".to_string()
    } else if target.starts_with("qwen") {
        "qwen".to_string()
    } else {
        target.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockProvider;
    use crate::providers::traits::{ChatMessage, Provider};
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::time::Duration;

    struct FailingProvider {
        name: String,
    }

    #[async_trait]
    impl Provider for FailingProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn available_models(&self) -> Vec<String> {
            vec!["broken-model".to_string()]
        }

        async fn chat(
            &self,
            _request: &crate::providers::traits::ChatRequest,
        ) -> anyhow::Result<crate::providers::traits::ChatResponse> {
            anyhow::bail!("simulated provider failure")
        }

        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![])
        }

        async fn validate_key(&self) -> anyhow::Result<bool> {
            Ok(true)
        }
    }

    fn sample_request() -> ChatRequest {
        ChatRequest {
            model: "mock-chat-1".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hello".to_string(),
            }],
            max_tokens: None,
            temperature: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn executes_preferred_provider_when_healthy() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("openai")));
        registry.register(Arc::new(MockProvider::new("anthropic")));

        let fallback = FallbackRouter::with_config(
            vec!["openai".to_string(), "anthropic".to_string()],
            2,
            Duration::from_secs(30),
        );
        let mut runtime = RoutingRuntime::new(registry, fallback);

        let decision = RouteDecision {
            model: crate::router::ModelId::GPT4oMini,
            target: RouteTarget::Cloud("openai-gpt-4o-mini".to_string()),
            reason: "test".to_string(),
            quality_score: 0.9,
            estimated_cost: 0.001,
        };

        let result = runtime.execute(&decision, &sample_request()).await.unwrap();
        assert_eq!(result.provider, "openai");
    }

    #[tokio::test]
    async fn falls_back_when_preferred_provider_fails() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(FailingProvider {
            name: "openai".to_string(),
        }));
        registry.register(Arc::new(MockProvider::new("anthropic")));

        let fallback = FallbackRouter::with_config(
            vec!["openai".to_string(), "anthropic".to_string()],
            1,
            Duration::from_secs(30),
        );
        let mut runtime = RoutingRuntime::new(registry, fallback);

        let decision = RouteDecision {
            model: crate::router::ModelId::GPT4oMini,
            target: RouteTarget::Cloud("openai-gpt-4o-mini".to_string()),
            reason: "test".to_string(),
            quality_score: 0.9,
            estimated_cost: 0.001,
        };

        let result = runtime.execute(&decision, &sample_request()).await.unwrap();
        assert_eq!(result.provider, "anthropic");
        assert_eq!(result.attempts.len(), 2);
        assert_eq!(result.attempts[0].provider, "openai");
        assert!(!result.attempts[0].success);
        assert_eq!(result.attempts[1].provider, "anthropic");
        assert!(result.attempts[1].success);
    }

    #[tokio::test]
    async fn returns_error_when_all_candidates_fail() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(FailingProvider {
            name: "openai".to_string(),
        }));

        let fallback =
            FallbackRouter::with_config(vec!["openai".to_string()], 1, Duration::from_secs(30));
        let mut runtime = RoutingRuntime::new(registry, fallback);

        let decision = RouteDecision {
            model: crate::router::ModelId::GPT4oMini,
            target: RouteTarget::Cloud("openai-gpt-4o-mini".to_string()),
            reason: "test".to_string(),
            quality_score: 0.9,
            estimated_cost: 0.001,
        };

        let result = runtime.execute(&decision, &sample_request()).await;
        assert!(result.is_err());
    }

    #[test]
    fn normalizes_provider_aliases() {
        assert_eq!(normalize_provider_name("openai-gpt-4o-mini"), "openai");
        assert_eq!(
            normalize_provider_name("anthropic-claude-sonnet"),
            "anthropic"
        );
        assert_eq!(normalize_provider_name("qwen2.5-7b-local"), "qwen");
    }
}
