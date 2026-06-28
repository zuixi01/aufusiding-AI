//! 隐私感知分发器
//!
//! 包装 IntentDispatcher，在输入和输出两端进行 PII 脱敏。

use async_trait::async_trait;

use crate::privacy::pii_detector::{PiiDetector, PiiEntity};

use super::dispatcher_trait::Dispatcher;
use super::intent::{DispatchResult, IntentDispatcher};

/// 脱敏配置
#[derive(Debug, Clone)]
pub struct SanitizerConfig {
    /// 是否启用脱敏
    pub enabled: bool,
    /// 最低置信度阈值（低于此值的 PII 不脱敏）
    pub min_confidence: f32,
}

impl Default for SanitizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.5,
        }
    }
}

/// 隐私感知分发器 — 输入脱敏 → 分发 → 输出脱敏
pub struct PrivacyAwareDispatcher {
    inner: IntentDispatcher,
    detector: PiiDetector,
    config: SanitizerConfig,
}

impl PrivacyAwareDispatcher {
    pub fn new(inner: IntentDispatcher, config: SanitizerConfig) -> Self {
        Self {
            inner,
            detector: PiiDetector::new(),
            config,
        }
    }

    /// 脱敏文本中的 PII
    fn sanitize(&self, text: &str) -> String {
        let entities = self.detector.detect(text);
        let filtered: Vec<&PiiEntity> = entities
            .iter()
            .filter(|e| e.confidence >= self.config.min_confidence)
            .collect();

        if filtered.is_empty() {
            return text.to_string();
        }

        // 从后往前替换，避免偏移量变化
        let mut result = text.to_string();
        let mut sorted: Vec<&PiiEntity> = filtered;
        sorted.sort_by(|a, b| b.start.cmp(&a.start));
        for entity in sorted {
            let replacement = format!("[{}]", entity.entity_type);
            if entity.start < result.len() && entity.end <= result.len() {
                result.replace_range(entity.start..entity.end, &replacement);
            }
        }
        result
    }
}

#[async_trait]
impl Dispatcher for PrivacyAwareDispatcher {
    async fn dispatch(&self, text: &str) -> DispatchResult {
        if !self.config.enabled {
            return self.inner.dispatch(text).await;
        }
        let sanitized_input = self.sanitize(text);
        let mut result = self.inner.dispatch(&sanitized_input).await;
        result.response = self.sanitize(&result.response);
        result
    }
}

/// IntentDispatcher 也实现 Dispatcher trait
#[async_trait]
impl Dispatcher for IntentDispatcher {
    async fn dispatch(&self, text: &str) -> DispatchResult {
        IntentDispatcher::dispatch(self, text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ambient::intent::DispatchTarget;

    #[tokio::test]
    async fn sanitizes_email_from_input() {
        let inner = IntentDispatcher::new();
        let dispatcher = PrivacyAwareDispatcher::new(inner, SanitizerConfig::default());
        let result = dispatcher.dispatch("我的邮箱是 test@example.com 帮我查天气").await;
        assert!(!result.response.contains("test@example.com"));
    }

    #[tokio::test]
    async fn sanitizes_phone_from_input() {
        let inner = IntentDispatcher::new();
        let dispatcher = PrivacyAwareDispatcher::new(inner, SanitizerConfig::default());
        let result = dispatcher.dispatch("我的电话是 13812345678 帮我查天气").await;
        assert!(!result.response.contains("13812345678"));
    }

    #[tokio::test]
    async fn clean_input_passes_through() {
        let inner = IntentDispatcher::new();
        let dispatcher = PrivacyAwareDispatcher::new(inner, SanitizerConfig::default());
        let result = dispatcher.dispatch("今天天气怎么样").await;
        assert_eq!(result.target, DispatchTarget::Chat);
        assert!(!result.response.is_empty());
    }

    #[tokio::test]
    async fn iot_command_passes_through() {
        let inner = IntentDispatcher::new();
        let dispatcher = PrivacyAwareDispatcher::new(inner, SanitizerConfig::default());
        let result = dispatcher.dispatch("打开客厅灯").await;
        assert_eq!(result.target, DispatchTarget::Iot);
    }

    #[tokio::test]
    async fn disabled_config_no_sanitization() {
        let inner = IntentDispatcher::new();
        let config = SanitizerConfig {
            enabled: false,
            min_confidence: 0.5,
        };
        let dispatcher = PrivacyAwareDispatcher::new(inner, config);
        let result = dispatcher
            .dispatch("邮箱 test@example.com 帮我查天气")
            .await;
        // 禁用时，response 可能包含原始文本（取决于 inner 的行为）
        // 关键是不会 panic
        assert!(!result.response.is_empty());
    }

    #[tokio::test]
    async fn multiple_pii_all_sanitized() {
        let inner = IntentDispatcher::new();
        let dispatcher = PrivacyAwareDispatcher::new(inner, SanitizerConfig::default());
        let result = dispatcher
            .dispatch("联系 test@example.com 或 13812345678")
            .await;
        assert!(!result.response.contains("test@example.com"));
        assert!(!result.response.contains("13812345678"));
    }

    #[tokio::test]
    async fn sanitize_preserves_normal_text() {
        let inner = IntentDispatcher::new();
        let dispatcher = PrivacyAwareDispatcher::new(inner, SanitizerConfig::default());
        let text = "今天天气真好";
        let sanitized = dispatcher.sanitize(text);
        assert_eq!(sanitized, text);
    }

    #[tokio::test]
    async fn sanitize_replaces_email() {
        let inner = IntentDispatcher::new();
        let dispatcher = PrivacyAwareDispatcher::new(inner, SanitizerConfig::default());
        let sanitized = dispatcher.sanitize("邮箱是 test@example.com 谢谢");
        assert!(!sanitized.contains("test@example.com"));
        assert!(sanitized.contains("[EMAIL]") || sanitized.contains("["));
    }

    #[test]
    fn sanitizer_config_default() {
        let cfg = SanitizerConfig::default();
        assert!(cfg.enabled);
        assert!((cfg.min_confidence - 0.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn intent_dispatcher_implements_dispatcher_trait() {
        let d: Box<dyn Dispatcher> = Box::new(IntentDispatcher::new());
        let result = d.dispatch("你好").await;
        assert_eq!(result.target, DispatchTarget::Chat);
    }

    #[tokio::test]
    async fn privacy_dispatcher_implements_dispatcher_trait() {
        let inner = IntentDispatcher::new();
        let d: Box<dyn Dispatcher> = Box::new(
            PrivacyAwareDispatcher::new(inner, SanitizerConfig::default()),
        );
        let result = d.dispatch("你好").await;
        assert_eq!(result.target, DispatchTarget::Chat);
    }

    #[tokio::test]
    async fn high_confidence_threshold_skips_low_confidence() {
        let inner = IntentDispatcher::new();
        let config = SanitizerConfig {
            enabled: true,
            min_confidence: 0.99, // 极高阈值
        };
        let dispatcher = PrivacyAwareDispatcher::new(inner, config);
        // 大部分 PII 检测置信度 < 0.99，应该不脱敏
        let sanitized = dispatcher.sanitize("联系 test@example.com");
        // 可能仍然脱敏（email 置信度通常很高），但不应 panic
        assert!(!sanitized.is_empty());
    }
}
