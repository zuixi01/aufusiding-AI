//! 集成测试：完整脱敏流程
//!
//! 验证 SanitizingProvider 端到端的 PII 脱敏与还原。

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    use crate::privacy::pii_detector::PiiType;
    use crate::privacy::sanitizer_config::SanitizerConfig;
    use crate::privacy::sanitizing_provider::SanitizingProvider;
    use crate::providers::traits::*;

    /// Mock Provider：捕获请求并回显含 token 的响应
    struct EchoMockProvider {
        captured: Mutex<Vec<ChatRequest>>,
    }

    impl EchoMockProvider {
        fn new() -> Self {
            Self {
                captured: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl Provider for EchoMockProvider {
        fn name(&self) -> &str {
            "echo-mock"
        }
        fn available_models(&self) -> Vec<String> {
            vec!["echo-1".into()]
        }
        async fn chat(&self, request: &ChatRequest) -> anyhow::Result<ChatResponse> {
            self.captured.lock().await.push(request.clone());
            // 回显最后一条消息（含 token）
            let last = request
                .messages
                .last()
                .map(|m| m.content.clone())
                .unwrap_or_default();
            Ok(ChatResponse {
                content: format!("收到: {last}"),
                model: "echo-1".into(),
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                },
            })
        }
        async fn embed(&self, _: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![])
        }
        async fn validate_key(&self) -> anyhow::Result<bool> {
            Ok(true)
        }
    }

    fn make_request(content: &str) -> ChatRequest {
        ChatRequest {
            model: "echo-1".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: content.into(),
            }],
            max_tokens: None,
            temperature: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn full_pipeline_email_sanitize_and_restore() {
        let mock = Arc::new(EchoMockProvider::new());
        let provider = SanitizingProvider::wrap(mock.clone(), SanitizerConfig::default());

        let request = make_request("请发送到 user@secret.com 这个邮箱");
        let response = provider.chat(&request).await.unwrap();

        // 验证 mock 收到的是脱敏数据
        let captured = mock.captured.lock().await;
        let sent = &captured[0].messages[0].content;
        assert!(!sent.contains("user@secret.com"), "PII leaked to provider");
        assert!(
            sent.contains("[EMAIL_"),
            "Token not present in sanitized request"
        );

        // 验证响应中 token 已还原
        assert!(
            response.content.contains("user@secret.com"),
            "PII not restored in response"
        );
    }

    #[tokio::test]
    async fn full_pipeline_phone_sanitize_and_restore() {
        let mock = Arc::new(EchoMockProvider::new());
        let provider = SanitizingProvider::wrap(mock.clone(), SanitizerConfig::default());

        let request = make_request("我的手机号是13912345678");
        let response = provider.chat(&request).await.unwrap();

        let captured = mock.captured.lock().await;
        let sent = &captured[0].messages[0].content;
        assert!(!sent.contains("13912345678"), "Phone leaked to provider");
        assert!(
            response.content.contains("13912345678"),
            "Phone not restored"
        );
    }

    #[tokio::test]
    async fn full_pipeline_mixed_pii() {
        let mock = Arc::new(EchoMockProvider::new());
        let provider = SanitizingProvider::wrap(mock.clone(), SanitizerConfig::default());

        let request = make_request("用户 admin@corp.com 手机 13800138000 IP 192.168.1.1");
        let response = provider.chat(&request).await.unwrap();

        let captured = mock.captured.lock().await;
        let sent = &captured[0].messages[0].content;
        assert!(!sent.contains("admin@corp.com"));
        assert!(!sent.contains("13800138000"));
        assert!(!sent.contains("192.168.1.1"));

        // 响应还原
        assert!(response.content.contains("admin@corp.com"));
        assert!(response.content.contains("13800138000"));
        assert!(response.content.contains("192.168.1.1"));
    }

    #[tokio::test]
    async fn multi_message_sanitization() {
        let mock = Arc::new(EchoMockProvider::new());
        let provider = SanitizingProvider::wrap(mock.clone(), SanitizerConfig::default());

        let request = ChatRequest {
            model: "echo-1".into(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: "你是助手".into(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: "邮箱 a@b.com".into(),
                },
                ChatMessage {
                    role: "assistant".into(),
                    content: "好的".into(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: "手机 13700137000".into(),
                },
            ],
            max_tokens: None,
            temperature: None,
            stream: false,
        };
        let _ = provider.chat(&request).await.unwrap();

        let captured = mock.captured.lock().await;
        assert!(!captured[0].messages[1].content.contains("a@b.com"));
        assert!(!captured[0].messages[3].content.contains("13700137000"));
        // system 和 assistant 消息无 PII，应不变
        assert_eq!(captured[0].messages[0].content, "你是助手");
        assert_eq!(captured[0].messages[2].content, "好的");
    }

    #[tokio::test]
    async fn type_filter_only_email() {
        let mock = Arc::new(EchoMockProvider::new());
        let config = SanitizerConfig::default().with_types(vec![PiiType::Email]);
        let provider = SanitizingProvider::wrap(mock.clone(), config);

        let request = make_request("邮箱 x@y.com 手机 13600136000");
        let _ = provider.chat(&request).await.unwrap();

        let captured = mock.captured.lock().await;
        let sent = &captured[0].messages[0].content;
        assert!(!sent.contains("x@y.com"), "Email should be sanitized");
        assert!(
            sent.contains("13600136000"),
            "Phone should pass through when not in filter"
        );
    }

    #[tokio::test]
    async fn error_propagation() {
        struct FailProvider;
        #[async_trait]
        impl Provider for FailProvider {
            fn name(&self) -> &str {
                "fail"
            }
            fn available_models(&self) -> Vec<String> {
                vec![]
            }
            async fn chat(&self, _: &ChatRequest) -> anyhow::Result<ChatResponse> {
                anyhow::bail!("provider error")
            }
            async fn embed(&self, _: &str) -> anyhow::Result<Vec<f32>> {
                anyhow::bail!("no")
            }
            async fn validate_key(&self) -> anyhow::Result<bool> {
                Ok(false)
            }
        }

        let provider = SanitizingProvider::wrap(Arc::new(FailProvider), SanitizerConfig::default());
        let result = provider.chat(&make_request("test@fail.com")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("provider error"));
    }
}
