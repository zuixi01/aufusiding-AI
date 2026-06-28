//! Slack 渠道适配器
//!
//! 通过 Slack API 收发消息。

use async_trait::async_trait;
use reqwest::Client;

/// Slack 渠道
pub struct SlackChannel {
    /// Bot Token
    bot_token: String,
    /// HTTP 客户端
    client: Client,
    /// API base URL（可覆盖用于测试）
    base_url: String,
}

impl SlackChannel {
    /// 创建 Slack 渠道
    pub fn new(bot_token: &str) -> Self {
        Self {
            bot_token: bot_token.to_string(),
            client: Client::new(),
            base_url: "https://slack.com/api".to_string(),
        }
    }

    /// 创建带自定义 base URL 的渠道（用于测试）
    pub fn with_base_url(bot_token: &str, base_url: &str) -> Self {
        Self {
            bot_token: bot_token.to_string(),
            client: Client::new(),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait]
impl super::traits::MessageChannel for SlackChannel {
    fn name(&self) -> &str {
        "slack"
    }

    async fn send(&self, recipient: &str, message: &super::traits::Message) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "channel": recipient,
            "text": message.text,
        });
        let resp = self
            .client
            .post(format!("{}/chat.postMessage", self.base_url))
            .bearer_auth(&self.bot_token)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Slack chat.postMessage failed: {}", resp.status());
        }
        Ok(())
    }

    async fn receive(&self) -> anyhow::Result<Vec<super::traits::ReceivedMessage>> {
        let resp = self
            .client
            .get(format!("{}/conversations.history", self.base_url))
            .bearer_auth(&self.bot_token)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Slack conversations.history failed: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let mut messages = Vec::new();
        if let Some(msgs) = body["messages"].as_array() {
            for msg in msgs {
                let text = msg["text"].as_str().unwrap_or_default();
                let user = msg["user"].as_str().unwrap_or_default();
                messages.push(super::traits::ReceivedMessage {
                    sender_id: user.to_string(),
                    message: super::traits::Message {
                        text: text.to_string(),
                        attachments: vec![],
                    },
                    timestamp: chrono::Utc::now(),
                });
            }
        }
        Ok(messages)
    }

    async fn is_available(&self) -> bool {
        self.client
            .post(format!("{}/auth.test", self.base_url))
            .bearer_auth(&self.bot_token)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
    }
}

#[cfg(test)]
mod tests {
    use super::super::traits::MessageChannel;
    use super::*;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn send_message_calls_chat_post_message() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/chat.postMessage");
            then.status(200).json_body(serde_json::json!({"ok": true}));
        });
        let channel = SlackChannel::with_base_url("xoxb-test", &server.base_url());
        let msg = super::super::traits::Message {
            text: "hello".to_string(),
            attachments: vec![],
        };
        channel.send("#general", &msg).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn send_includes_bearer_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/chat.postMessage")
                .header("Authorization", "Bearer xoxb-secret");
            then.status(200).json_body(serde_json::json!({"ok": true}));
        });
        let channel = SlackChannel::with_base_url("xoxb-secret", &server.base_url());
        let msg = super::super::traits::Message {
            text: "auth test".to_string(),
            attachments: vec![],
        };
        channel.send("#test", &msg).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn receive_parses_conversations_history() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/conversations.history");
            then.status(200).json_body(serde_json::json!({
                "ok": true,
                "messages": [
                    {"text": "hello slack", "user": "U123"}
                ]
            }));
        });
        let channel = SlackChannel::with_base_url("xoxb-test", &server.base_url());
        let msgs = channel.receive().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message.text, "hello slack");
        assert_eq!(msgs[0].sender_id, "U123");
    }

    #[tokio::test]
    async fn receive_empty_channel() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/conversations.history");
            then.status(200)
                .json_body(serde_json::json!({"ok": true, "messages": []}));
        });
        let channel = SlackChannel::with_base_url("xoxb-test", &server.base_url());
        let msgs = channel.receive().await.unwrap();
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn is_available_valid_token() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/auth.test");
            then.status(200).json_body(serde_json::json!({"ok": true}));
        });
        let channel = SlackChannel::with_base_url("xoxb-test", &server.base_url());
        assert!(channel.is_available().await);
    }

    #[tokio::test]
    async fn is_available_invalid_token() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/auth.test");
            then.status(401);
        });
        let channel = SlackChannel::with_base_url("bad-token", &server.base_url());
        assert!(!channel.is_available().await);
    }
}
