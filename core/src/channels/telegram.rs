//! Telegram 渠道适配器
//!
//! 通过 Telegram Bot API 收发消息。

use async_trait::async_trait;
use reqwest::Client;

/// Telegram 渠道
pub struct TelegramChannel {
    /// Bot Token
    bot_token: String,
    /// HTTP 客户端
    client: Client,
    /// API base URL（可覆盖用于测试）
    base_url: String,
}

impl TelegramChannel {
    /// 创建 Telegram 渠道
    pub fn new(bot_token: &str) -> Self {
        Self {
            bot_token: bot_token.to_string(),
            client: Client::new(),
            base_url: "https://api.telegram.org".to_string(),
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

    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, self.bot_token, method)
    }
}

#[async_trait]
impl super::traits::MessageChannel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn send(&self, recipient: &str, message: &super::traits::Message) -> anyhow::Result<()> {
        let mut body = serde_json::json!({
            "chat_id": recipient,
            "text": message.text,
        });
        if !message.attachments.is_empty() {
            body["attachments"] = serde_json::json!(message.attachments);
        }
        let resp = self
            .client
            .post(self.api_url("sendMessage"))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Telegram sendMessage failed: {}", resp.status());
        }
        Ok(())
    }

    async fn receive(&self) -> anyhow::Result<Vec<super::traits::ReceivedMessage>> {
        let resp = self.client.get(self.api_url("getUpdates")).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Telegram getUpdates failed: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let mut messages = Vec::new();
        if let Some(results) = body["result"].as_array() {
            for update in results {
                if let Some(msg) = update.get("message") {
                    let text = msg["text"].as_str().unwrap_or_default();
                    let from_id = msg["from"]["id"].as_i64().unwrap_or_default();
                    messages.push(super::traits::ReceivedMessage {
                        sender_id: from_id.to_string(),
                        message: super::traits::Message {
                            text: text.to_string(),
                            attachments: vec![],
                        },
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }
        Ok(messages)
    }

    async fn is_available(&self) -> bool {
        self.client
            .get(self.api_url("getMe"))
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
    async fn send_message_calls_api() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path_includes("sendMessage");
            then.status(200).json_body(serde_json::json!({"ok": true}));
        });
        let channel = TelegramChannel::with_base_url("test-token", &server.base_url());
        let msg = super::super::traits::Message {
            text: "hello".to_string(),
            attachments: vec![],
        };
        channel.send("12345", &msg).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn send_with_attachments() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path_includes("sendMessage");
            then.status(200).json_body(serde_json::json!({"ok": true}));
        });
        let channel = TelegramChannel::with_base_url("test-token", &server.base_url());
        let msg = super::super::traits::Message {
            text: "see file".to_string(),
            attachments: vec!["https://example.com/f.png".to_string()],
        };
        channel.send("12345", &msg).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn receive_parses_updates() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path_includes("getUpdates");
            then.status(200).json_body(serde_json::json!({
                "ok": true,
                "result": [{
                    "update_id": 1,
                    "message": {
                        "text": "hi there",
                        "from": {"id": 42}
                    }
                }]
            }));
        });
        let channel = TelegramChannel::with_base_url("test-token", &server.base_url());
        let msgs = channel.receive().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message.text, "hi there");
        assert_eq!(msgs[0].sender_id, "42");
    }

    #[tokio::test]
    async fn receive_empty_returns_empty() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path_includes("getUpdates");
            then.status(200)
                .json_body(serde_json::json!({"ok": true, "result": []}));
        });
        let channel = TelegramChannel::with_base_url("test-token", &server.base_url());
        let msgs = channel.receive().await.unwrap();
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn is_available_valid_token() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path_includes("getMe");
            then.status(200).json_body(serde_json::json!({"ok": true}));
        });
        let channel = TelegramChannel::with_base_url("test-token", &server.base_url());
        assert!(channel.is_available().await);
    }

    #[tokio::test]
    async fn is_available_invalid_token() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path_includes("getMe");
            then.status(401);
        });
        let channel = TelegramChannel::with_base_url("bad-token", &server.base_url());
        assert!(!channel.is_available().await);
    }
}
