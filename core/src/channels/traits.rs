//! 消息渠道 trait
//!
//! 定义统一的消息渠道接口，所有渠道适配器必须实现此 trait。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 消息内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息文本
    pub text: String,
    /// 附件 URL（可选）
    pub attachments: Vec<String>,
}

/// 消息渠道 trait
#[async_trait]
pub trait MessageChannel: Send + Sync {
    /// 渠道名称
    fn name(&self) -> &str;

    /// 发送消息
    async fn send(&self, recipient: &str, message: &Message) -> anyhow::Result<()>;

    /// 接收消息（轮询或 webhook）
    async fn receive(&self) -> anyhow::Result<Vec<ReceivedMessage>>;

    /// 检查渠道是否可用
    async fn is_available(&self) -> bool;
}

/// 接收到的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedMessage {
    /// 发送者 ID
    pub sender_id: String,
    /// 消息内容
    pub message: Message,
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 测试用 Mock 消息渠道
pub struct MockMessageChannel {
    channel_name: String,
    send_log: std::sync::Mutex<Vec<(String, Message)>>,
    receive_messages: std::sync::Mutex<Vec<ReceivedMessage>>,
    available: std::sync::atomic::AtomicBool,
}

impl MockMessageChannel {
    pub fn new(name: &str) -> Self {
        Self {
            channel_name: name.to_string(),
            send_log: std::sync::Mutex::new(Vec::new()),
            receive_messages: std::sync::Mutex::new(Vec::new()),
            available: std::sync::atomic::AtomicBool::new(true),
        }
    }

    pub fn add_incoming(&self, msg: ReceivedMessage) {
        self.receive_messages.lock().unwrap_or_else(|e| e.into_inner()).push(msg);
    }

    pub fn sent_messages(&self) -> Vec<(String, Message)> {
        self.send_log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_available(&self, val: bool) {
        self.available.store(val, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl MessageChannel for MockMessageChannel {
    fn name(&self) -> &str {
        &self.channel_name
    }

    async fn send(&self, recipient: &str, message: &Message) -> anyhow::Result<()> {
        self.send_log.lock().unwrap_or_else(|e| e.into_inner()).push((recipient.to_string(), message.clone()));
        Ok(())
    }

    async fn receive(&self) -> anyhow::Result<Vec<ReceivedMessage>> {
        Ok(self.receive_messages.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    async fn is_available(&self) -> bool {
        self.available.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_creation() {
        let msg = Message {
            text: "hello".to_string(),
            attachments: vec!["https://example.com/file.png".to_string()],
        };
        assert_eq!(msg.text, "hello");
        assert_eq!(msg.attachments.len(), 1);
    }

    #[test]
    fn received_message_has_timestamp() {
        let msg = ReceivedMessage {
            sender_id: "user-1".to_string(),
            message: Message {
                text: "hi".to_string(),
                attachments: vec![],
            },
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(msg.sender_id, "user-1");
        assert!(msg.timestamp <= chrono::Utc::now());
    }

    #[test]
    fn message_with_attachments() {
        let msg = Message {
            text: "see attached".to_string(),
            attachments: vec![
                "https://example.com/a.png".to_string(),
                "https://example.com/b.pdf".to_string(),
            ],
        };
        assert_eq!(msg.attachments.len(), 2);
    }

    #[test]
    fn empty_message() {
        let msg = Message {
            text: String::new(),
            attachments: vec![],
        };
        assert!(msg.text.is_empty());
        assert!(msg.attachments.is_empty());
    }

    #[tokio::test]
    async fn mock_channel_send_records() {
        let ch = MockMessageChannel::new("test");
        let msg = Message { text: "hi".into(), attachments: vec![] };
        ch.send("user1", &msg).await.unwrap();
        let sent = ch.sent_messages();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "user1");
        assert_eq!(sent[0].1.text, "hi");
    }

    #[tokio::test]
    async fn mock_channel_receive() {
        let ch = MockMessageChannel::new("test");
        ch.add_incoming(ReceivedMessage {
            sender_id: "s1".into(),
            message: Message { text: "hello".into(), attachments: vec![] },
            timestamp: chrono::Utc::now(),
        });
        let msgs = ch.receive().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender_id, "s1");
    }

    #[tokio::test]
    async fn mock_channel_availability() {
        let ch = MockMessageChannel::new("test");
        assert!(ch.is_available().await);
        ch.set_available(false);
        assert!(!ch.is_available().await);
    }

    #[test]
    fn mock_channel_name() {
        let ch = MockMessageChannel::new("slack");
        assert_eq!(ch.name(), "slack");
    }
}
