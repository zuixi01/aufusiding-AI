//! REST 邮件提供者
//!
//! 通过 Gmail/Outlook 风格 REST API 实现 EmailProvider trait。
//! 使用 reqwest::Client + 可配置 base_url，便于 httpmock 测试。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::email_client::{EmailAttachment, EmailMessage, EmailProvider, EmailSearchQuery};

// ─── API 响应类型 ────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ApiMessageList {
    messages: Vec<ApiMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiMessage {
    id: String,
    from: String,
    to: Vec<String>,
    subject: String,
    body: String,
    date: DateTime<Utc>,
    #[serde(default)]
    attachments: Vec<ApiAttachment>,
    #[serde(default)]
    is_read: bool,
    thread_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiAttachment {
    filename: String,
    mime_type: String,
    size_bytes: u64,
}

#[derive(Debug, Serialize)]
struct SendRequest {
    to: Vec<String>,
    subject: String,
    body: String,
}

#[derive(Debug, Serialize)]
struct ReplyRequest {
    body: String,
}

#[derive(Debug, Deserialize)]
struct SendResponse {
    id: String,
}

// ─── RestEmailProvider ──────────────────────────────────

/// REST API 邮件提供者（Gmail/Outlook 风格）
pub struct RestEmailProvider {
    access_token: String,
    client: Client,
    base_url: String,
}

impl RestEmailProvider {
    pub fn new(access_token: &str) -> Self {
        Self {
            access_token: access_token.to_string(),
            client: Client::new(),
            base_url: "https://api.mail.example.com/v1".to_string(),
        }
    }

    /// 创建带自定义 base URL 的实例（用于测试）
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        Self {
            access_token: access_token.to_string(),
            client: Client::new(),
            base_url: base_url.to_string(),
        }
    }

    fn convert_message(api: ApiMessage) -> EmailMessage {
        let id = Uuid::parse_str(&api.id).unwrap_or_else(|_| Uuid::new_v4());
        EmailMessage {
            id,
            from: api.from,
            to: api.to,
            subject: api.subject,
            body: api.body,
            date: api.date,
            attachments: api
                .attachments
                .into_iter()
                .map(|a| EmailAttachment {
                    filename: a.filename,
                    mime_type: a.mime_type,
                    size_bytes: a.size_bytes,
                })
                .collect(),
            is_read: api.is_read,
            thread_id: api.thread_id,
        }
    }
}

#[async_trait]
impl EmailProvider for RestEmailProvider {
    async fn fetch_inbox(&self, limit: usize) -> anyhow::Result<Vec<EmailMessage>> {
        let resp = self
            .client
            .get(format!("{}/messages?limit={}", self.base_url, limit))
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("fetch_inbox failed: {}", resp.status());
        }
        let list: ApiMessageList = resp.json().await?;
        Ok(list
            .messages
            .into_iter()
            .map(Self::convert_message)
            .collect())
    }

    async fn search(&self, query: &EmailSearchQuery) -> anyhow::Result<Vec<EmailMessage>> {
        let mut url = format!("{}/messages/search?", self.base_url);
        if let Some(kw) = &query.keyword {
            url.push_str(&format!("q={}&", kw));
        }
        if let Some(sender) = &query.sender {
            url.push_str(&format!("from={}&", sender));
        }
        if query.unread_only {
            url.push_str("is_read=false&");
        }
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("search failed: {}", resp.status());
        }
        let list: ApiMessageList = resp.json().await?;
        Ok(list
            .messages
            .into_iter()
            .map(Self::convert_message)
            .collect())
    }

    async fn get_message(&self, id: &Uuid) -> anyhow::Result<EmailMessage> {
        let resp = self
            .client
            .get(format!("{}/messages/{}", self.base_url, id))
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("get_message failed: {}", resp.status());
        }
        let api: ApiMessage = resp.json().await?;
        Ok(Self::convert_message(api))
    }

    async fn send(&self, to: &[String], subject: &str, body: &str) -> anyhow::Result<Uuid> {
        let req = SendRequest {
            to: to.to_vec(),
            subject: subject.to_string(),
            body: body.to_string(),
        };
        let resp = self
            .client
            .post(format!("{}/messages", self.base_url))
            .bearer_auth(&self.access_token)
            .json(&req)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("send failed: {}", resp.status());
        }
        let send_resp: SendResponse = resp.json().await?;
        let id = Uuid::parse_str(&send_resp.id).unwrap_or_else(|_| Uuid::new_v4());
        Ok(id)
    }

    async fn reply(&self, message_id: &Uuid, body: &str) -> anyhow::Result<Uuid> {
        let req = ReplyRequest {
            body: body.to_string(),
        };
        let resp = self
            .client
            .post(format!("{}/messages/{}/reply", self.base_url, message_id))
            .bearer_auth(&self.access_token)
            .json(&req)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("reply failed: {}", resp.status());
        }
        let send_resp: SendResponse = resp.json().await?;
        let id = Uuid::parse_str(&send_resp.id).unwrap_or_else(|_| Uuid::new_v4());
        Ok(id)
    }

    async fn mark_read(&self, id: &Uuid) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/messages/{}/read", self.base_url, id))
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("mark_read failed: {}", resp.status());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    const TEST_TOKEN: &str = "ya29.test-access-token";
    const TEST_MSG_ID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn mock_api_message(id: &str, subject: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "from": "alice@example.com",
            "to": ["bob@example.com"],
            "subject": subject,
            "body": "Hello from REST",
            "date": "2026-03-17T10:00:00Z",
            "attachments": [],
            "is_read": false,
            "thread_id": null
        })
    }

    #[tokio::test]
    async fn fetch_inbox_returns_messages() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/messages")
                .query_param("limit", "10");
            then.status(200).json_body(serde_json::json!({
                "messages": [mock_api_message(TEST_MSG_ID, "Weekly Report")]
            }));
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        let msgs = provider.fetch_inbox(10).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].subject, "Weekly Report");
    }

    #[tokio::test]
    async fn fetch_inbox_includes_bearer_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/messages")
                .header("Authorization", &format!("Bearer {}", TEST_TOKEN));
            then.status(200)
                .json_body(serde_json::json!({"messages": []}));
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        provider.fetch_inbox(5).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn search_with_keyword() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/messages/search")
                .query_param("q", "urgent");
            then.status(200).json_body(serde_json::json!({
                "messages": [mock_api_message(TEST_MSG_ID, "Urgent: Review")]
            }));
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        let query = EmailSearchQuery {
            keyword: Some("urgent".into()),
            ..Default::default()
        };
        let results = provider.search(&query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].subject, "Urgent: Review");
    }

    #[tokio::test]
    async fn get_message_by_id() {
        let id = Uuid::parse_str(TEST_MSG_ID).unwrap();
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path(format!("/messages/{}", TEST_MSG_ID));
            then.status(200)
                .json_body(mock_api_message(TEST_MSG_ID, "Hello"));
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        let msg = provider.get_message(&id).await.unwrap();
        assert_eq!(msg.subject, "Hello");
        assert_eq!(msg.id, id);
    }

    #[tokio::test]
    async fn send_email_posts_to_messages() {
        let reply_id = "660e8400-e29b-41d4-a716-446655440001";
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/messages");
            then.status(201)
                .json_body(serde_json::json!({"id": reply_id}));
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        let id = provider
            .send(&["bob@example.com".into()], "Test", "Body")
            .await
            .unwrap();
        mock.assert();
        assert_eq!(id, Uuid::parse_str(reply_id).unwrap());
    }

    #[tokio::test]
    async fn reply_posts_to_message_reply() {
        let original_id = Uuid::parse_str(TEST_MSG_ID).unwrap();
        let reply_id = "770e8400-e29b-41d4-a716-446655440002";
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path(format!("/messages/{}/reply", TEST_MSG_ID));
            then.status(201)
                .json_body(serde_json::json!({"id": reply_id}));
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        let id = provider.reply(&original_id, "Thanks!").await.unwrap();
        mock.assert();
        assert_eq!(id, Uuid::parse_str(reply_id).unwrap());
    }

    #[tokio::test]
    async fn mark_read_posts_to_read_endpoint() {
        let id = Uuid::parse_str(TEST_MSG_ID).unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path(format!("/messages/{}/read", TEST_MSG_ID));
            then.status(200);
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        provider.mark_read(&id).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn fetch_inbox_server_error() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/messages");
            then.status(500);
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        assert!(provider.fetch_inbox(10).await.is_err());
    }

    #[tokio::test]
    async fn get_message_not_found() {
        let id = Uuid::new_v4();
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path_contains("/messages/");
            then.status(404);
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        assert!(provider.get_message(&id).await.is_err());
    }

    #[tokio::test]
    async fn search_unread_only() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/messages/search")
                .query_param("is_read", "false");
            then.status(200)
                .json_body(serde_json::json!({"messages": []}));
        });
        let provider = RestEmailProvider::with_base_url(TEST_TOKEN, &server.base_url());
        let query = EmailSearchQuery {
            unread_only: true,
            ..Default::default()
        };
        let results = provider.search(&query).await.unwrap();
        mock.assert();
        assert!(results.is_empty());
    }
}
