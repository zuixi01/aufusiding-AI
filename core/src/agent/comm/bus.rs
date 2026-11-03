//! AgentBus — Agent 消息总线
//!
//! 基于 tokio broadcast channel，支持消息广播、历史记录和过滤订阅。

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, RwLock};

use crate::agent::types::AgentId;

use super::protocol::{AgentEnvelope, MessageFilter};

/// Agent 消息总线
pub struct AgentBus {
    sender: broadcast::Sender<AgentEnvelope>,
    history: Arc<RwLock<Vec<AgentEnvelope>>>,
    filters: Arc<RwLock<HashMap<AgentId, Vec<MessageFilter>>>>,
}

impl AgentBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            history: Arc::new(RwLock::new(Vec::new())),
            filters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 发送消息到总线
    pub async fn send(&self, envelope: AgentEnvelope) {
        self.history.write().await.push(envelope.clone());
        if self.sender.send(envelope).is_err() {
            tracing::debug!("agent bus: no active receivers");
        }
    }

    /// 订阅总线（原始，无过滤）
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEnvelope> {
        self.sender.subscribe()
    }

    /// 为 Agent 注册消息过滤器
    pub async fn register_filters(&self, agent_id: AgentId, filters: Vec<MessageFilter>) {
        self.filters.write().await.insert(agent_id, filters);
    }

    /// 注销 Agent 的过滤器
    pub async fn unregister_filters(&self, agent_id: &AgentId) {
        self.filters.write().await.remove(agent_id);
    }

    /// 获取历史消息（指定时间之后的）
    pub async fn get_history(&self, since: DateTime<Utc>) -> Vec<AgentEnvelope> {
        let history = self.history.read().await;
        history
            .iter()
            .filter(|e| e.timestamp >= since)
            .cloned()
            .collect()
    }

    /// 获取全部历史消息
    pub async fn get_all_history(&self) -> Vec<AgentEnvelope> {
        self.history.read().await.clone()
    }

    /// 历史消息数量
    pub async fn history_len(&self) -> usize {
        self.history.read().await.len()
    }

    /// 清空历史
    pub async fn clear_history(&self) {
        self.history.write().await.clear();
    }

    /// 当前订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// 检查消息是否匹配某 Agent 的过滤器
    pub async fn matches_filters(&self, agent_id: &AgentId, envelope: &AgentEnvelope) -> bool {
        let filters = self.filters.read().await;
        match filters.get(agent_id) {
            Some(fs) => fs.iter().any(|f| f.matches(envelope, agent_id)),
            None => true, // 无过滤器 = 接收所有
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::comm::protocol::{AgentMessage, MessageTarget};

    fn make_bus() -> AgentBus {
        AgentBus::new(64)
    }

    fn shutdown_envelope(from: &AgentId) -> AgentEnvelope {
        AgentEnvelope::new(
            from.clone(),
            MessageTarget::Broadcast,
            AgentMessage::Shutdown,
        )
    }

    #[tokio::test]
    async fn send_records_history() {
        let bus = make_bus();
        let from = AgentId::new();
        bus.send(shutdown_envelope(&from)).await;
        bus.send(shutdown_envelope(&from)).await;
        assert_eq!(bus.history_len().await, 2);
    }

    #[tokio::test]
    async fn subscribe_receives_messages() {
        let bus = make_bus();
        let mut rx = bus.subscribe();
        let from = AgentId::new();
        bus.send(shutdown_envelope(&from)).await;
        let env = rx.recv().await.unwrap();
        assert_eq!(env.from, from);
    }

    #[tokio::test]
    async fn get_history_filters_by_time() {
        let bus = make_bus();
        let from = AgentId::new();
        bus.send(shutdown_envelope(&from)).await;
        let after = Utc::now();
        // 小延迟确保时间戳不同
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        bus.send(shutdown_envelope(&from)).await;
        let recent = bus.get_history(after).await;
        assert_eq!(recent.len(), 1);
    }

    #[tokio::test]
    async fn clear_history_works() {
        let bus = make_bus();
        let from = AgentId::new();
        bus.send(shutdown_envelope(&from)).await;
        assert_eq!(bus.history_len().await, 1);
        bus.clear_history().await;
        assert_eq!(bus.history_len().await, 0);
    }

    #[tokio::test]
    async fn subscriber_count_tracks() {
        let bus = make_bus();
        assert_eq!(bus.subscriber_count(), 0);
        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);
        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);
    }

    #[tokio::test]
    async fn matches_filters_with_registered_filter() {
        let bus = make_bus();
        let agent = AgentId::new();
        bus.register_filters(
            agent.clone(),
            vec![MessageFilter::MessageKind("shutdown".into())],
        )
        .await;

        let env_shutdown = shutdown_envelope(&AgentId::new());
        assert!(bus.matches_filters(&agent, &env_shutdown).await);

        let env_question = AgentEnvelope::new(
            AgentId::new(),
            MessageTarget::Broadcast,
            AgentMessage::Question {
                content: "hi".into(),
            },
        );
        assert!(!bus.matches_filters(&agent, &env_question).await);
    }

    #[tokio::test]
    async fn no_filters_means_accept_all() {
        let bus = make_bus();
        let agent = AgentId::new();
        let env = shutdown_envelope(&AgentId::new());
        assert!(bus.matches_filters(&agent, &env).await);
    }
}
