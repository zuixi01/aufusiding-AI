//! 微信渠道适配器
//!
//! 通过企业微信 API 收发消息。

use async_trait::async_trait;
use reqwest::Client;

/// 微信渠道
pub struct WeChatChannel {
    /// 企业微信凭证（corpid:corpsecret）
    credentials: String,
    /// HTTP 客户端
    client: Client,
    /// API base URL（可覆盖用于测试）
    base_url: String,
    /// 缓存的 access_token
    access_token: Option<String>,
}

impl WeChatChannel {
    /// 创建微信渠道
    pub fn new(credentials: &str) -> Self {
        Self {
            credentials: credentials.to_string(),
            client: Client::new(),
            base_url: "https://qyapi.weixin.qq.com/cgi-bin".to_string(),
            access_token: None,
        }
    }

    /// 创建带自定义 base URL 的渠道（用于测试）
    pub fn with_base_url(credentials: &str, base_url: &str) -> Self {
        Self {
            credentials: credentials.to_string(),
            client: Client::new(),
            base_url: base_url.to_string(),
            access_token: None,
        }
    }

    /// 设置 access_token（用于测试）
    pub fn set_access_token(&mut self, token: &str) {
        self.access_token = Some(token.to_string());
    }
}

#[async_trait]
impl super::traits::MessageChannel for WeChatChannel {
    fn name(&self) -> &str {
        "wechat"
    }

    async fn send(&self, recipient: &str, message: &super::traits::Message) -> anyhow::Result<()> {
        let token = self
            .access_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no access_token set"))?;
        let body = serde_json::json!({
            "touser": recipient,
            "msgtype": "text",
            "agentid": 1,
            "text": { "content": message.text },
        });
        let resp = self
            .client
            .post(format!(
                "{}/message/send?access_token={}",
                self.base_url, token
            ))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("WeChat message/send failed: {}", resp.status());
        }
        Ok(())
    }

    async fn receive(&self) -> anyhow::Result<Vec<super::traits::ReceivedMessage>> {
        let token = self
            .access_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no access_token set"))?;
        let resp = self
            .client
            .get(format!(
                "{}/message/receive?access_token={}",
                self.base_url, token
            ))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("WeChat message/receive failed: {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let mut messages = Vec::new();
        if let Some(msgs) = body["messages"].as_array() {
            for msg in msgs {
                let text = msg["content"].as_str().unwrap_or_default();
                let from = msg["from_user"].as_str().unwrap_or_default();
                messages.push(super::traits::ReceivedMessage {
                    sender_id: from.to_string(),
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
        let parts: Vec<&str> = self.credentials.splitn(2, ':').collect();
        if parts.len() != 2 {
            return false;
        }
        let url = format!(
            "{}/gettoken?corpid={}&corpsecret={}",
            self.base_url, parts[0], parts[1]
        );
        self.client
            .get(url)
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

    fn channel_with_token(server: &MockServer) -> WeChatChannel {
        let mut ch = WeChatChannel::with_base_url("corpid:corpsecret", &server.base_url());
        ch.set_access_token("test-token");
        ch
    }

    #[tokio::test]
    async fn send_message_calls_api() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/message/send");
            then.status(200)
                .json_body(serde_json::json!({"errcode": 0}));
        });
        let channel = channel_with_token(&server);
        let msg = super::super::traits::Message {
            text: "hello".to_string(),
            attachments: vec![],
        };
        channel.send("user1", &msg).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn send_includes_access_token() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/message/send")
                .query_param("access_token", "test-token");
            then.status(200)
                .json_body(serde_json::json!({"errcode": 0}));
        });
        let channel = channel_with_token(&server);
        let msg = super::super::traits::Message {
            text: "token test".to_string(),
            attachments: vec![],
        };
        channel.send("user1", &msg).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn receive_parses_callback() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/message/receive");
            then.status(200).json_body(serde_json::json!({
                "messages": [
                    {"content": "你好", "from_user": "zhangsan"}
                ]
            }));
        });
        let channel = channel_with_token(&server);
        let msgs = channel.receive().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message.text, "你好");
        assert_eq!(msgs[0].sender_id, "zhangsan");
    }

    #[tokio::test]
    async fn receive_empty() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/message/receive");
            then.status(200)
                .json_body(serde_json::json!({"messages": []}));
        });
        let channel = channel_with_token(&server);
        let msgs = channel.receive().await.unwrap();
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn is_available_valid_credentials() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/gettoken");
            then.status(200)
                .json_body(serde_json::json!({"errcode": 0, "access_token": "tk"}));
        });
        let channel = WeChatChannel::with_base_url("corpid:corpsecret", &server.base_url());
        assert!(channel.is_available().await);
    }

    #[tokio::test]
    async fn is_available_expired_token() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/gettoken");
            then.status(401);
        });
        let channel = WeChatChannel::with_base_url("corpid:corpsecret", &server.base_url());
        assert!(!channel.is_available().await);
    }
}
