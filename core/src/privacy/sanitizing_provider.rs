//! SanitizingProvider — Provider Decorator
//!
//! 在 Provider trait 边界拦截所有云端请求，自动脱敏 PII 并还原响应。
//! 军工级模式集成 DataClassifier、多策略引擎、审计日志和密钥管理。

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::providers::traits::{ChatRequest, ChatResponse, Provider};
use crate::safety::classification::{DataClassifier, SensitivityLevel};
use super::audit_log::{AuditOperation, SanitizationAuditLog};
use super::context_leak_detector::ContextLeakDetector;
use super::key_manager::{KeyManager, KeyPurpose};
use super::pii_detector::PiiDetector;
use super::sanitization_policy::SanitizationPolicy;
use super::sanitization_strategy::{self, SanitizationStrategy, StrategyKind};
use super::sanitizer_config::SanitizerConfig;
use super::sensitive_dictionary::SensitiveDictionary;
use super::token_vault::TokenVault;

/// Provider Decorator：自动脱敏 PII
pub struct SanitizingProvider {
    inner: Arc<dyn Provider>,
    detector: PiiDetector,
    vault: Mutex<TokenVault>,
    config: SanitizerConfig,
    policy: SanitizationPolicy,
    strategies: HashMap<StrategyKind, Box<dyn SanitizationStrategy>>,
    audit_log: Option<Arc<SanitizationAuditLog>>,
    #[allow(dead_code)]
    key_manager: Option<Arc<KeyManager>>,
}

impl SanitizingProvider {
    /// 包装一个 Provider，注入脱敏层（向后兼容）
    pub fn wrap(inner: Arc<dyn Provider>, config: SanitizerConfig) -> Self {
        let strategies = Self::build_default_strategies(None);
        let detector = Self::build_detector(&config);
        Self {
            inner,
            detector,
            vault: Mutex::new(TokenVault::new()),
            policy: SanitizationPolicy::new(config.classification_enabled),
            strategies,
            audit_log: None,
            key_manager: None,
            config,
        }
    }

    /// 军工级构造器：一键启用所有强化特性
    pub fn wrap_military(
        inner: Arc<dyn Provider>,
        audit_log: Arc<SanitizationAuditLog>,
        key_manager: Arc<KeyManager>,
    ) -> anyhow::Result<Self> {
        let config = SanitizerConfig::military_grade();
        // 从 key_manager 获取 FPE 密钥
        let fpe_key = key_manager
            .get_active_key(KeyPurpose::Sanitization)?
            .map(|(_, k)| k);
        let strategies = Self::build_default_strategies(fpe_key.as_deref());
        let vault = TokenVault::new_with_ttl(config.vault_ttl);
        let detector = Self::build_detector(&config);

        Ok(Self {
            inner,
            detector,
            vault: Mutex::new(vault),
            policy: SanitizationPolicy::new(true),
            strategies,
            audit_log: Some(audit_log),
            key_manager: Some(key_manager),
            config,
        })
    }

    /// 带自定义词典路径的构造器
    pub fn wrap_with_dictionaries(
        inner: Arc<dyn Provider>,
        config: SanitizerConfig,
    ) -> Self {
        let strategies = Self::build_default_strategies(None);
        let detector = Self::build_detector(&config);
        Self {
            inner,
            detector,
            vault: Mutex::new(TokenVault::new()),
            policy: SanitizationPolicy::new(config.classification_enabled),
            strategies,
            audit_log: None,
            key_manager: None,
            config,
        }
    }

    /// 根据配置构建 PiiDetector（含词典 + 上下文检测器）
    fn build_detector(config: &SanitizerConfig) -> PiiDetector {
        let mut detector = PiiDetector::new();

        // 加载外部词典
        if !config.dictionary_paths.is_empty() {
            match SensitiveDictionary::load_from_paths(&config.dictionary_paths) {
                Ok(dict) => {
                    detector = detector.with_dictionary(Arc::new(dict));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to load dictionaries, continuing without");
                }
            }
        }

        // 启用上下文泄露检测
        if config.context_leak_detection {
            let ctx = ContextLeakDetector::new(config.custom_env_prefixes.clone());
            detector = detector.with_context_detector(ctx);
        }

        detector
    }

    fn build_default_strategies(fpe_key: Option<&[u8]>) -> HashMap<StrategyKind, Box<dyn SanitizationStrategy>> {
        let mut map: HashMap<StrategyKind, Box<dyn SanitizationStrategy>> = HashMap::new();
        map.insert(StrategyKind::Tokenize, sanitization_strategy::create_strategy(StrategyKind::Tokenize, None));
        map.insert(StrategyKind::StaticMask, sanitization_strategy::create_strategy(StrategyKind::StaticMask, None));
        map.insert(StrategyKind::PartialMask, sanitization_strategy::create_strategy(StrategyKind::PartialMask, None));
        map.insert(StrategyKind::Redact, sanitization_strategy::create_strategy(StrategyKind::Redact, None));
        map.insert(StrategyKind::Fpe, sanitization_strategy::create_strategy(StrategyKind::Fpe, fpe_key));
        map
    }

    /// 对文本执行 PII 检测 + 策略分派脱敏
    ///
    /// 返回 `None` 表示 block_on_high_risk 触发，应阻止发送。
    fn sanitize_text(&self, text: &str, vault: &mut TokenVault) -> Option<String> {
        let entities = self.detector.detect(text);
        if entities.is_empty() {
            return Some(text.to_string());
        }
        // 过滤：按配置的类型和置信度
        let filtered: Vec<_> = entities
            .into_iter()
            .filter(|e| {
                self.config.is_type_enabled(&e.entity_type)
                    && e.confidence >= self.config.min_confidence
            })
            .collect();

        if filtered.is_empty() {
            return Some(text.to_string());
        }

        // 分级（只调用一次 DataClassifier）
        let classification = if self.config.classification_enabled {
            Some(DataClassifier::classify(text))
        } else {
            None
        };

        // block_on_high_risk：L3 机密数据直接阻止发送
        if self.config.block_on_high_risk {
            if let Some(ref cls) = classification {
                if cls.level == SensitivityLevel::Confidential {
                    if self.config.log_detections {
                        tracing::warn!(
                            level = ?cls.level,
                            "High-risk data blocked from sending"
                        );
                    }
                    if let Some(ref audit) = self.audit_log {
                        if let Err(e) = audit.log_operation(
                            AuditOperation::Sanitize,
                            "BLOCKED",
                            "block_on_high_risk",
                            cls.level as u8,
                            "[blocked]",
                        ) {
                            tracing::warn!("audit log write failed: {}", e);
                        }
                    }
                    return None;
                }
            }
        }

        // 策略分派
        let sensitivity_level = classification.as_ref().map(|c| c.level as u8).unwrap_or(1);
        let decisions = if let Some(ref cls) = classification {
            self.policy.resolve(cls, &filtered)
        } else {
            filtered.iter().map(|e| {
                super::sanitization_policy::PolicyDecision {
                    entity: e.clone(),
                    strategy: self.config.default_strategy,
                }
            }).collect()
        };

        // 从后往前替换，避免偏移错乱
        let mut sorted_decisions = decisions;
        sorted_decisions.sort_by(|a, b| b.entity.start.cmp(&a.entity.start));

        let mut result = text.to_string();
        for decision in &sorted_decisions {
            let strategy = self.strategies
                .get(&decision.strategy)
                .expect("strategy not registered");
            let replacement = strategy.sanitize(&decision.entity, vault);

            if self.config.log_detections {
                tracing::info!(
                    pii_type = %decision.entity.entity_type,
                    strategy = %decision.strategy,
                    "PII detected and sanitized"
                );
            }

            // 审计日志
            if let Some(ref audit) = self.audit_log {
                if let Err(e) = audit.log_operation(
                    AuditOperation::Sanitize,
                    &decision.entity.entity_type.to_string(),
                    &decision.strategy.to_string(),
                    sensitivity_level,
                    &decision.entity.text,
                ) {
                    tracing::warn!("audit log write failed: {}", e);
                }
            }

            result.replace_range(decision.entity.start..decision.entity.end, &replacement);
        }
        Some(result)
    }
}

#[async_trait]
impl Provider for SanitizingProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn available_models(&self) -> Vec<String> {
        self.inner.available_models()
    }

    async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse> {
        if !self.config.enabled {
            return self.inner.chat(request).await;
        }

        // 1. 深拷贝 request（不可变原则）
        let mut sanitized = request.clone();

        // 2. 对每条 message.content 执行脱敏
        {
            let mut vault = self.vault.lock().await;
            for msg in &mut sanitized.messages {
                match self.sanitize_text(&msg.content, &mut vault) {
                    Some(sanitized_content) => msg.content = sanitized_content,
                    None => return Err(anyhow::anyhow!(
                        "Request blocked: high-risk confidential data detected"
                    )),
                }
            }
        }

        // 3. 调用内部 provider
        let mut response = self.inner.chat(&sanitized).await?;

        // 4. 还原响应中的 token
        {
            let vault = self.vault.lock().await;
            response.content = vault.detokenize(&response.content);
        }

        Ok(response)
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        if !self.config.enabled {
            return self.inner.embed(text).await;
        }
        let sanitized = {
            let mut vault = self.vault.lock().await;
            match self.sanitize_text(text, &mut vault) {
                Some(s) => s,
                None => return Err(anyhow::anyhow!(
                    "Request blocked: high-risk confidential data detected"
                )),
            }
        };
        self.inner.embed(&sanitized).await
    }

    async fn validate_key(&self) -> anyhow::Result<bool> {
        self.inner.validate_key().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::traits::{ChatMessage, ChatRequest, ChatResponse, TokenUsage};

    struct CaptureMockProvider {
        captured: Mutex<Vec<ChatRequest>>,
    }

    impl CaptureMockProvider {
        fn new() -> Self {
            Self { captured: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl Provider for CaptureMockProvider {
        fn name(&self) -> &str { "capture-mock" }
        fn available_models(&self) -> Vec<String> { vec!["mock-1".into()] }
        async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse> {
            self.captured.lock().await.push(request.clone());
            Ok(ChatResponse {
                content: format!("echo: {}", request.messages.last().map(|m| m.content.as_str()).unwrap_or("")),
                model: "mock-1".into(),
                usage: TokenUsage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
            })
        }
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> { Ok(vec![0.0]) }
        async fn validate_key(&self) -> anyhow::Result<bool> { Ok(true) }
    }

    #[tokio::test]
    async fn sanitizes_pii_in_request() {
        let mock = Arc::new(CaptureMockProvider::new());
        let provider = SanitizingProvider::wrap(mock.clone(), SanitizerConfig::default());
        let request = ChatRequest {
            model: "mock-1".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "我的邮箱是 test@example.com".into(),
            }],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        let _ = provider.chat(&request).await.unwrap();
        let captured = mock.captured.lock().await;
        let sent_content = &captured[0].messages[0].content;
        assert!(!sent_content.contains("test@example.com"), "PII should be sanitized");
        assert!(sent_content.contains("[EMAIL_"), "Should contain email token");
    }

    #[tokio::test]
    async fn no_pii_passthrough() {
        let mock = Arc::new(CaptureMockProvider::new());
        let provider = SanitizingProvider::wrap(mock.clone(), SanitizerConfig::default());
        let request = ChatRequest {
            model: "mock-1".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "今天天气怎么样".into(),
            }],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        let _ = provider.chat(&request).await.unwrap();
        let captured = mock.captured.lock().await;
        assert_eq!(captured[0].messages[0].content, "今天天气怎么样");
    }

    #[tokio::test]
    async fn disabled_config_passthrough() {
        let mock = Arc::new(CaptureMockProvider::new());
        let mut config = SanitizerConfig::default();
        config.enabled = false;
        let provider = SanitizingProvider::wrap(mock.clone(), config);
        let request = ChatRequest {
            model: "mock-1".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "邮箱 test@example.com".into(),
            }],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        let _ = provider.chat(&request).await.unwrap();
        let captured = mock.captured.lock().await;
        assert!(captured[0].messages[0].content.contains("test@example.com"));
    }

    #[tokio::test]
    async fn delegates_name_and_models() {
        let mock = Arc::new(CaptureMockProvider::new());
        let provider = SanitizingProvider::wrap(mock, SanitizerConfig::default());
        assert_eq!(provider.name(), "capture-mock");
        assert_eq!(provider.available_models(), vec!["mock-1".to_string()]);
    }

    #[tokio::test]
    async fn block_on_high_risk_rejects_confidential_data() {
        let mock = Arc::new(CaptureMockProvider::new());
        let mut config = SanitizerConfig::default();
        config.block_on_high_risk = true;
        config.classification_enabled = true;
        let provider = SanitizingProvider::wrap(mock.clone(), config);
        // 包含密码模式的文本会被 DataClassifier 标记为 L3 机密
        let request = ChatRequest {
            model: "mock-1".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "我的密码是 password=SuperSecret123! 和 api_key=sk-abc123def456".into(),
            }],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        let result = provider.chat(&request).await;
        assert!(result.is_err(), "Should block high-risk data");
        assert!(result.unwrap_err().to_string().contains("blocked"));
        // 确认请求没有发送到内部 provider
        let captured = mock.captured.lock().await;
        assert!(captured.is_empty(), "No request should reach inner provider");
    }
}
