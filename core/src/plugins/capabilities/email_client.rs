//! 邮件客户端
//!
//! 统一邮件收发接口，支持搜索、回复、待办提取、语音摘要。
//! 通过 EmailProvider trait 抽象不同邮件服务（IMAP/Gmail/Outlook）。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── 核心类型 ────────────────────────────────────────────

/// 邮件附件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAttachment {
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

/// 邮件消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub id: Uuid,
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
    pub date: DateTime<Utc>,
    pub attachments: Vec<EmailAttachment>,
    pub is_read: bool,
    pub thread_id: Option<String>,
}

/// 邮件搜索查询
#[derive(Debug, Clone, Default)]
pub struct EmailSearchQuery {
    pub keyword: Option<String>,
    pub sender: Option<String>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub unread_only: bool,
}

// ─── EmailProvider trait ─────────────────────────────────

#[async_trait]
pub trait EmailProvider: Send + Sync {
    async fn fetch_inbox(&self, limit: usize) -> anyhow::Result<Vec<EmailMessage>>;
    async fn search(&self, query: &EmailSearchQuery) -> anyhow::Result<Vec<EmailMessage>>;
    async fn get_message(&self, id: &Uuid) -> anyhow::Result<EmailMessage>;
    async fn send(&self, to: &[String], subject: &str, body: &str) -> anyhow::Result<Uuid>;
    async fn reply(&self, message_id: &Uuid, body: &str) -> anyhow::Result<Uuid>;
    async fn mark_read(&self, id: &Uuid) -> anyhow::Result<()>;
}

/// Mock 邮件服务
pub struct MockEmailProvider {
    messages: Mutex<HashMap<Uuid, EmailMessage>>,
}

impl MockEmailProvider {
    pub fn new() -> Self {
        Self {
            messages: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_message(&self, msg: EmailMessage) {
        self.messages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(msg.id, msg);
    }
}

#[async_trait]
impl EmailProvider for MockEmailProvider {
    async fn fetch_inbox(&self, limit: usize) -> anyhow::Result<Vec<EmailMessage>> {
        let msgs = self.messages.lock().unwrap_or_else(|e| e.into_inner());
        let mut list: Vec<EmailMessage> = msgs.values().cloned().collect();
        list.sort_by_key(|message| std::cmp::Reverse(message.date));
        list.truncate(limit);
        Ok(list)
    }

    async fn search(&self, query: &EmailSearchQuery) -> anyhow::Result<Vec<EmailMessage>> {
        let msgs = self.messages.lock().unwrap_or_else(|e| e.into_inner());
        let results: Vec<EmailMessage> = msgs
            .values()
            .filter(|m| {
                if query.unread_only && m.is_read {
                    return false;
                }
                if let Some(kw) = &query.keyword {
                    if !m.subject.contains(kw) && !m.body.contains(kw) {
                        return false;
                    }
                }
                if let Some(sender) = &query.sender {
                    if !m.from.contains(sender) {
                        return false;
                    }
                }
                if let Some(from) = query.date_from {
                    if m.date < from {
                        return false;
                    }
                }
                if let Some(to) = query.date_to {
                    if m.date > to {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        Ok(results)
    }

    async fn get_message(&self, id: &Uuid) -> anyhow::Result<EmailMessage> {
        let msgs = self.messages.lock().unwrap_or_else(|e| e.into_inner());
        msgs.get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("message not found: {}", id))
    }

    async fn send(&self, to: &[String], subject: &str, body: &str) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        let msg = EmailMessage {
            id,
            from: "me@example.com".into(),
            to: to.to_vec(),
            subject: subject.into(),
            body: body.into(),
            date: Utc::now(),
            attachments: Vec::new(),
            is_read: true,
            thread_id: None,
        };
        self.messages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, msg);
        Ok(id)
    }

    async fn reply(&self, message_id: &Uuid, body: &str) -> anyhow::Result<Uuid> {
        let original = self.get_message(message_id).await?;
        let id = Uuid::new_v4();
        let msg = EmailMessage {
            id,
            from: "me@example.com".into(),
            to: vec![original.from],
            subject: format!("Re: {}", original.subject),
            body: body.into(),
            date: Utc::now(),
            attachments: Vec::new(),
            is_read: true,
            thread_id: original.thread_id,
        };
        self.messages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, msg);
        Ok(id)
    }

    async fn mark_read(&self, id: &Uuid) -> anyhow::Result<()> {
        let mut msgs = self.messages.lock().unwrap_or_else(|e| e.into_inner());
        let msg = msgs
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("message not found: {}", id))?;
        msg.is_read = true;
        Ok(())
    }
}

// ─── EmailClient ─────────────────────────────────────────

/// 邮件客户端
pub struct EmailClient {
    provider: Box<dyn EmailProvider>,
}

impl EmailClient {
    pub fn new(provider: Box<dyn EmailProvider>) -> Self {
        Self { provider }
    }

    pub async fn fetch_inbox(&self, limit: usize) -> anyhow::Result<Vec<EmailMessage>> {
        self.provider.fetch_inbox(limit).await
    }

    pub async fn search_by_keyword(&self, keyword: &str) -> anyhow::Result<Vec<EmailMessage>> {
        self.provider
            .search(&EmailSearchQuery {
                keyword: Some(keyword.to_string()),
                ..Default::default()
            })
            .await
    }

    pub async fn search_by_sender(&self, sender: &str) -> anyhow::Result<Vec<EmailMessage>> {
        self.provider
            .search(&EmailSearchQuery {
                sender: Some(sender.to_string()),
                ..Default::default()
            })
            .await
    }

    pub async fn unread_count(&self) -> anyhow::Result<usize> {
        let results = self
            .provider
            .search(&EmailSearchQuery {
                unread_only: true,
                ..Default::default()
            })
            .await?;
        Ok(results.len())
    }

    pub async fn get_message(&self, id: &Uuid) -> anyhow::Result<EmailMessage> {
        self.provider.get_message(id).await
    }

    pub async fn send(&self, to: &[String], subject: &str, body: &str) -> anyhow::Result<Uuid> {
        self.provider.send(to, subject, body).await
    }

    pub async fn reply(&self, message_id: &Uuid, body: &str) -> anyhow::Result<Uuid> {
        self.provider.reply(message_id, body).await
    }

    pub async fn mark_read(&self, id: &Uuid) -> anyhow::Result<()> {
        self.provider.mark_read(id).await
    }

    /// 从邮件正文提取待办关键词
    pub fn extract_action_items(message: &EmailMessage) -> Vec<String> {
        let keywords = [
            "请",
            "需要",
            "务必",
            "deadline",
            "TODO",
            "action required",
            "请回复",
            "请确认",
            "提交",
            "审批",
        ];
        let mut items = Vec::new();
        for line in message.body.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            for kw in &keywords {
                if trimmed.to_lowercase().contains(&kw.to_lowercase()) {
                    items.push(trimmed.to_string());
                    break;
                }
            }
        }
        items
    }

    /// 语音摘要
    pub async fn voice_summary(&self) -> anyhow::Result<String> {
        let unread = self
            .provider
            .search(&EmailSearchQuery {
                unread_only: true,
                ..Default::default()
            })
            .await?;

        if unread.is_empty() {
            return Ok("没有未读邮件。".into());
        }

        let count = unread.len();
        // group by sender domain
        let mut sender_counts: HashMap<String, usize> = HashMap::new();
        for msg in &unread {
            let domain = msg.from.split('@').nth(1).unwrap_or("未知");
            *sender_counts.entry(domain.to_string()).or_insert(0) += 1;
        }

        let top_sender = sender_counts
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(d, c)| format!("其中 {} 封来自{}", c, d))
            .unwrap_or_default();

        Ok(format!("你有 {} 封未读邮件，{}。", count, top_sender))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_msg(subject: &str, from: &str, is_read: bool) -> EmailMessage {
        EmailMessage {
            id: Uuid::new_v4(),
            from: from.into(),
            to: vec!["me@example.com".into()],
            subject: subject.into(),
            body: "请确认相关事项。".into(),
            date: Utc::now(),
            attachments: Vec::new(),
            is_read,
            thread_id: None,
        }
    }

    fn setup_provider() -> MockEmailProvider {
        let p = MockEmailProvider::new();
        p.add_message(sample_msg("周报", "boss@company.com", false));
        p.add_message(sample_msg("会议通知", "hr@company.com", false));
        p.add_message(sample_msg("已读邮件", "friend@gmail.com", true));
        p
    }

    fn setup_client() -> EmailClient {
        EmailClient::new(Box::new(setup_provider()))
    }

    #[tokio::test]
    async fn fetch_inbox() {
        let client = setup_client();
        let msgs = client.fetch_inbox(10).await.unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[tokio::test]
    async fn fetch_inbox_limit() {
        let client = setup_client();
        let msgs = client.fetch_inbox(1).await.unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[tokio::test]
    async fn unread_count() {
        let client = setup_client();
        assert_eq!(client.unread_count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn search_by_keyword() {
        let client = setup_client();
        let results = client.search_by_keyword("周报").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].subject, "周报");
    }

    #[tokio::test]
    async fn search_by_sender() {
        let client = setup_client();
        let results = client.search_by_sender("hr@").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn send_email() {
        let client = setup_client();
        let id = client
            .send(&["bob@example.com".into()], "测试", "内容")
            .await
            .unwrap();
        let msg = client.get_message(&id).await.unwrap();
        assert_eq!(msg.subject, "测试");
    }

    #[tokio::test]
    async fn reply_email() {
        let client = setup_client();
        let inbox = client.fetch_inbox(1).await.unwrap();
        let original_id = inbox[0].id;
        let reply_id = client.reply(&original_id, "收到，谢谢").await.unwrap();
        let reply = client.get_message(&reply_id).await.unwrap();
        assert!(reply.subject.starts_with("Re: "));
    }

    #[tokio::test]
    async fn mark_read() {
        let provider = setup_provider();
        let msgs: Vec<EmailMessage> = provider
            .messages
            .lock()
            .unwrap()
            .values()
            .filter(|m| !m.is_read)
            .cloned()
            .collect();
        let id = msgs[0].id;
        let client = EmailClient::new(Box::new(provider));
        client.mark_read(&id).await.unwrap();
        let msg = client.get_message(&id).await.unwrap();
        assert!(msg.is_read);
    }

    #[tokio::test]
    async fn get_message_not_found() {
        let client = setup_client();
        assert!(client.get_message(&Uuid::new_v4()).await.is_err());
    }

    #[test]
    fn extract_action_items_finds_keywords() {
        let msg = EmailMessage {
            id: Uuid::new_v4(),
            from: "boss@co.com".into(),
            to: vec!["me@co.com".into()],
            subject: "任务".into(),
            body: "请确认本周报告。\n普通内容\n需要在周五前提交。".into(),
            date: Utc::now(),
            attachments: Vec::new(),
            is_read: false,
            thread_id: None,
        };
        let items = EmailClient::extract_action_items(&msg);
        assert_eq!(items.len(), 2);
        assert!(items[0].contains("请确认"));
        assert!(items[1].contains("需要"));
    }

    #[test]
    fn extract_action_items_empty_body() {
        let msg = EmailMessage {
            id: Uuid::new_v4(),
            from: "a@b.com".into(),
            to: vec![],
            subject: "hi".into(),
            body: "hello world".into(),
            date: Utc::now(),
            attachments: Vec::new(),
            is_read: true,
            thread_id: None,
        };
        assert!(EmailClient::extract_action_items(&msg).is_empty());
    }

    #[tokio::test]
    async fn voice_summary_with_unread() {
        let client = setup_client();
        let summary = client.voice_summary().await.unwrap();
        assert!(summary.contains("2 封未读"));
        assert!(summary.contains("company.com"));
    }

    #[tokio::test]
    async fn voice_summary_no_unread() {
        let provider = MockEmailProvider::new();
        provider.add_message(sample_msg("已读", "a@b.com", true));
        let client = EmailClient::new(Box::new(provider));
        let summary = client.voice_summary().await.unwrap();
        assert!(summary.contains("没有未读"));
    }

    #[test]
    fn email_message_serde_roundtrip() {
        let msg = sample_msg("测试", "a@b.com", false);
        let json = serde_json::to_string(&msg).unwrap();
        let back: EmailMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.subject, "测试");
    }

    #[test]
    fn email_attachment_serde() {
        let att = EmailAttachment {
            filename: "report.pdf".into(),
            mime_type: "application/pdf".into(),
            size_bytes: 1024,
        };
        let json = serde_json::to_string(&att).unwrap();
        let back: EmailAttachment = serde_json::from_str(&json).unwrap();
        assert_eq!(back.filename, "report.pdf");
    }
}
