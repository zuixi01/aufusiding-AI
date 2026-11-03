//! HTTP 聊天后端
//!
//! 通过 OpenAI 兼容的 `/v1/chat/completions` API 实现 ChatBackend，
//! 将 Message 上下文转换为 API 格式并解析响应。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::orchestrator::{BackendReply, ChatBackend};
use crate::providers::traits::TokenUsage;
use crate::router::classifier::ClassificationResult;
use crate::router::strategy::RouteDecision;
use crate::session::history::{Message, Role};

/// HTTP 聊天后端配置
#[derive(Debug, Clone)]
pub struct HttpChatConfig {
    /// API base URL (e.g. "https://api.openai.com")
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 模型名称 (e.g. "gpt-4o")
    pub model_name: String,
}

/// HTTP 聊天后端，实现 OpenAI 兼容的 chat completions API
pub struct HttpChatBackend {
    config: HttpChatConfig,
    client: Client,
}

impl HttpChatBackend {
    /// 创建新的 HTTP 聊天后端
    pub fn new(config: HttpChatConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 创建带自定义 base URL 的后端（用于测试）
    pub fn with_base_url(api_key: &str, model_name: &str, base_url: &str) -> Self {
        Self {
            config: HttpChatConfig {
                base_url: base_url.to_string(),
                api_key: api_key.to_string(),
                model_name: model_name.to_string(),
            },
            client: Client::new(),
        }
    }

    /// 将 Message 上下文转换为 API 消息格式
    fn convert_messages(context: &[Message], input: &str) -> Vec<ApiMessage> {
        let mut api_messages: Vec<ApiMessage> = context
            .iter()
            .map(|msg| ApiMessage {
                role: match msg.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => "system".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: msg.content.clone(),
            })
            .collect();

        api_messages.push(ApiMessage {
            role: "user".to_string(),
            content: input.to_string(),
        });

        api_messages
    }
}

/// API 消息格式
#[derive(Debug, Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    content: String,
}

/// Chat completions 请求体
#[derive(Debug, Serialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<ApiMessage>,
}

/// Chat completions 响应体
#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ApiMessage,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[async_trait]
impl ChatBackend for HttpChatBackend {
    async fn chat(
        &self,
        input: &str,
        context: &[Message],
        _classification: &ClassificationResult,
        _decision: &RouteDecision,
    ) -> anyhow::Result<BackendReply> {
        let messages = Self::convert_messages(context, input);

        let body = ChatCompletionsRequest {
            model: self.config.model_name.clone(),
            messages,
        };

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("chat completions API failed ({}): {}", status, text);
        }

        let api_resp: ChatCompletionsResponse = resp.json().await?;
        let content = api_resp
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        let token_usage = api_resp.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });
        let model = api_resp.model;

        Ok(BackendReply {
            content,
            route_attempts: vec!["http-chat-completions".to_string()],
            token_usage,
            provider: Some("http".to_string()),
            model,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::capability::ModelId;
    use crate::router::classifier::IntentType;
    use crate::router::strategy::{RouteDecision, RouteTarget};
    use httpmock::prelude::*;

    fn stub_classification() -> ClassificationResult {
        ClassificationResult {
            primary: IntentType::Knowledge,
            confidence: 0.9,
            complexity: 3,
        }
    }

    fn stub_decision() -> RouteDecision {
        RouteDecision {
            model: ModelId::GPT4o,
            target: RouteTarget::Cloud("gpt-4o".to_string()),
            reason: "test".to_string(),
            quality_score: 0.9,
            estimated_cost: 0.001,
        }
    }

    fn mock_chat_response(content: &str, total: usize) -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": total - 10,
                "total_tokens": total
            }
        })
    }

    #[tokio::test]
    async fn chat_sends_correct_request_and_parses_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("Authorization", "Bearer test-key");
            then.status(200).json_body(mock_chat_response("Hello!", 20));
        });

        let backend = HttpChatBackend::with_base_url("test-key", "gpt-4o", &server.base_url());
        let reply = backend
            .chat("Hi", &[], &stub_classification(), &stub_decision())
            .await
            .unwrap();

        assert_eq!(reply.content, "Hello!");
        assert_eq!(reply.token_usage.as_ref().unwrap().total_tokens, 20);
        assert_eq!(reply.model.as_deref(), Some("gpt-4o"));
        mock.assert();
    }

    #[tokio::test]
    async fn chat_converts_context_messages() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .body_contains(r#""role":"system""#);
            then.status(200)
                .json_body(mock_chat_response("response", 15));
        });

        let context = vec![Message {
            id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            role: Role::System,
            content: "You are helpful.".to_string(),
            timestamp: chrono::Utc::now(),
        }];

        let backend = HttpChatBackend::with_base_url("key", "model", &server.base_url());
        let reply = backend
            .chat("hello", &context, &stub_classification(), &stub_decision())
            .await
            .unwrap();

        assert_eq!(reply.content, "response");
        mock.assert();
    }

    #[tokio::test]
    async fn chat_handles_api_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(500).body("internal server error");
        });

        let backend = HttpChatBackend::with_base_url("key", "model", &server.base_url());
        let result = backend
            .chat("test", &[], &stub_classification(), &stub_decision())
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("500"));
    }

    #[tokio::test]
    async fn chat_includes_bearer_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("Authorization", "Bearer sk-my-secret");
            then.status(200).json_body(mock_chat_response("ok", 10));
        });

        let backend = HttpChatBackend::with_base_url("sk-my-secret", "gpt-4o", &server.base_url());
        backend
            .chat("hi", &[], &stub_classification(), &stub_decision())
            .await
            .unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn chat_empty_choices_returns_empty_content() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200).json_body(serde_json::json!({
                "id": "chatcmpl-empty",
                "object": "chat.completion",
                "model": "gpt-4o",
                "choices": [],
                "usage": null
            }));
        });

        let backend = HttpChatBackend::with_base_url("key", "model", &server.base_url());
        let reply = backend
            .chat("test", &[], &stub_classification(), &stub_decision())
            .await
            .unwrap();

        assert_eq!(reply.content, "");
        assert!(reply.token_usage.is_none());
    }
}
