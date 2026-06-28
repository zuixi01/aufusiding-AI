//! 云端 API 适配器模块
//!
//! 统一的云端 LLM API 接口，支持 OpenAI、Anthropic、Qwen 等多家 Provider。

pub mod anthropic;
pub mod local_llama;
pub mod mock;
pub mod ollama;
pub mod openai;
pub mod qwen;
pub mod registry;
pub mod traits;

pub use traits::Provider;

#[cfg(test)]
mod tests {
    use super::mock::MockProvider;
    use super::registry::ProviderRegistry;
    use std::sync::Arc;

    #[test]
    fn register_and_lookup_provider() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("mock-a")));

        assert!(registry.get("mock-a").is_some());
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn unregister_provider() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("mock-a")));
        registry.register(Arc::new(MockProvider::new("mock-b")));

        let removed = registry.unregister("mock-a");
        assert!(removed.is_some());
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.list_names(), vec!["mock-b".to_string()]);
    }

    #[test]
    fn register_duplicate_overwrites() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("mock-a")));
        let old = registry.register(Arc::new(MockProvider::new("mock-a")));
        assert!(old.is_some());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let registry = ProviderRegistry::new();
        assert!(registry.get("does-not-exist").is_none());
    }

    #[test]
    fn list_names_sorted() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("charlie")));
        registry.register(Arc::new(MockProvider::new("alpha")));
        registry.register(Arc::new(MockProvider::new("bravo")));
        assert_eq!(
            registry.list_names(),
            vec![
                "alpha".to_string(),
                "bravo".to_string(),
                "charlie".to_string()
            ]
        );
    }
}
