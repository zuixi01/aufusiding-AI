//! 1:1 直接消息 — Agent 之间的点对点通信

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

use crate::agent::types::AgentId;

use super::protocol::AgentEnvelope;

/// 直接消息信箱
pub struct DirectMailbox {
    inboxes: Arc<RwLock<HashMap<AgentId, mpsc::UnboundedSender<AgentEnvelope>>>>,
}

impl DirectMailbox {
    pub fn new() -> Self {
        Self {
            inboxes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 为 Agent 注册信箱，返回接收端
    pub async fn register(&self, agent_id: AgentId) -> mpsc::UnboundedReceiver<AgentEnvelope> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inboxes.write().await.insert(agent_id, tx);
        rx
    }

    /// 注销信箱
    pub async fn unregister(&self, agent_id: &AgentId) {
        self.inboxes.write().await.remove(agent_id);
    }

    /// 发送直接消息给指定 Agent
    pub async fn send(&self, to: &AgentId, envelope: AgentEnvelope) -> Result<(), SendError> {
        let inboxes = self.inboxes.read().await;
        let tx = inboxes.get(to).ok_or(SendError::RecipientNotFound)?;
        tx.send(envelope).map_err(|_| SendError::ChannelClosed)
    }

    /// 检查 Agent 是否已注册信箱
    pub async fn has_inbox(&self, agent_id: &AgentId) -> bool {
        self.inboxes.read().await.contains_key(agent_id)
    }

    /// 已注册信箱数量
    pub async fn inbox_count(&self) -> usize {
        self.inboxes.read().await.len()
    }
}

/// 直接消息发送错误
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("recipient not found")]
    RecipientNotFound,
    #[error("channel closed")]
    ChannelClosed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::comm::protocol::{AgentMessage, MessageTarget};

    fn make_envelope(from: &AgentId, to: &AgentId) -> AgentEnvelope {
        AgentEnvelope::new(
            from.clone(),
            MessageTarget::Agent(to.clone()),
            AgentMessage::Question {
                content: "hello?".into(),
            },
        )
    }

    #[tokio::test]
    async fn register_and_send() {
        let mailbox = DirectMailbox::new();
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();

        let mut rx = mailbox.register(agent_b.clone()).await;
        let env = make_envelope(&agent_a, &agent_b);
        mailbox.send(&agent_b, env).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.from, agent_a);
    }

    #[tokio::test]
    async fn send_to_unregistered_fails() {
        let mailbox = DirectMailbox::new();
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        let env = make_envelope(&agent_a, &agent_b);
        let result = mailbox.send(&agent_b, env).await;
        assert!(matches!(result, Err(SendError::RecipientNotFound)));
    }

    #[tokio::test]
    async fn unregister_removes_inbox() {
        let mailbox = DirectMailbox::new();
        let agent = AgentId::new();
        let _rx = mailbox.register(agent.clone()).await;
        assert!(mailbox.has_inbox(&agent).await);
        mailbox.unregister(&agent).await;
        assert!(!mailbox.has_inbox(&agent).await);
    }

    #[tokio::test]
    async fn inbox_count() {
        let mailbox = DirectMailbox::new();
        assert_eq!(mailbox.inbox_count().await, 0);
        let a = AgentId::new();
        let b = AgentId::new();
        let _rx1 = mailbox.register(a).await;
        let _rx2 = mailbox.register(b).await;
        assert_eq!(mailbox.inbox_count().await, 2);
    }
}
