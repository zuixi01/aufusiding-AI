//! N:N 群组讨论 — 圆桌会议等多 Agent 群聊

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::agent::types::AgentId;

use super::bus::AgentBus;
use super::protocol::{AgentEnvelope, AgentMessage, MessageTarget};

/// 群组 ID
pub type GroupId = String;

/// 群组信息
#[derive(Debug, Clone)]
pub struct Group {
    pub id: GroupId,
    pub topic: String,
    pub members: HashSet<AgentId>,
}

/// 群组管理器 — 管理所有讨论组
pub struct GroupManager {
    groups: Arc<RwLock<HashMap<GroupId, Group>>>,
    bus: Arc<AgentBus>,
}

impl GroupManager {
    pub fn new(bus: Arc<AgentBus>) -> Self {
        Self {
            groups: Arc::new(RwLock::new(HashMap::new())),
            bus,
        }
    }

    /// 创建讨论组
    pub async fn create_group(
        &self,
        id: impl Into<String>,
        topic: impl Into<String>,
        members: Vec<AgentId>,
    ) -> GroupId {
        let id = id.into();
        let group = Group {
            id: id.clone(),
            topic: topic.into(),
            members: members.into_iter().collect(),
        };
        self.groups.write().await.insert(id.clone(), group);
        id
    }

    /// 向群组发送消息（通过 bus 广播，目标为 Group）
    pub async fn send_to_group(
        &self,
        group_id: &str,
        from: AgentId,
        payload: AgentMessage,
    ) -> Result<(), GroupError> {
        let groups = self.groups.read().await;
        if !groups.contains_key(group_id) {
            return Err(GroupError::NotFound(group_id.to_string()));
        }
        let envelope = AgentEnvelope::new(from, MessageTarget::Group(group_id.to_string()), payload);
        self.bus.send(envelope).await;
        Ok(())
    }

    /// 添加成员
    pub async fn add_member(&self, group_id: &str, agent_id: AgentId) -> Result<(), GroupError> {
        let mut groups = self.groups.write().await;
        let group = groups
            .get_mut(group_id)
            .ok_or_else(|| GroupError::NotFound(group_id.to_string()))?;
        group.members.insert(agent_id);
        Ok(())
    }

    /// 移除成员
    pub async fn remove_member(
        &self,
        group_id: &str,
        agent_id: &AgentId,
    ) -> Result<(), GroupError> {
        let mut groups = self.groups.write().await;
        let group = groups
            .get_mut(group_id)
            .ok_or_else(|| GroupError::NotFound(group_id.to_string()))?;
        group.members.remove(agent_id);
        Ok(())
    }

    /// 获取群组信息
    pub async fn get_group(&self, group_id: &str) -> Option<Group> {
        self.groups.read().await.get(group_id).cloned()
    }

    /// 检查 Agent 是否是群组成员
    pub async fn is_member(&self, group_id: &str, agent_id: &AgentId) -> bool {
        self.groups
            .read()
            .await
            .get(group_id)
            .map(|g| g.members.contains(agent_id))
            .unwrap_or(false)
    }

    /// 解散群组
    pub async fn dissolve_group(&self, group_id: &str) -> Result<(), GroupError> {
        self.groups
            .write()
            .await
            .remove(group_id)
            .map(|_| ())
            .ok_or_else(|| GroupError::NotFound(group_id.to_string()))
    }

    /// 列出所有群组
    pub async fn list_groups(&self) -> Vec<GroupId> {
        self.groups.read().await.keys().cloned().collect()
    }

    /// 群组数量
    pub async fn group_count(&self) -> usize {
        self.groups.read().await.len()
    }
}

/// 群组操作错误
#[derive(Debug, thiserror::Error)]
pub enum GroupError {
    #[error("group not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::comm::protocol::AgentMessage;

    fn make_group_manager() -> GroupManager {
        let bus = Arc::new(AgentBus::new(64));
        GroupManager::new(bus)
    }

    #[tokio::test]
    async fn create_and_get_group() {
        let gm = make_group_manager();
        let a = AgentId::new();
        let b = AgentId::new();
        gm.create_group("rt-1", "架构讨论", vec![a.clone(), b.clone()])
            .await;
        let group = gm.get_group("rt-1").await.unwrap();
        assert_eq!(group.topic, "架构讨论");
        assert_eq!(group.members.len(), 2);
    }

    #[tokio::test]
    async fn add_and_remove_member() {
        let gm = make_group_manager();
        let a = AgentId::new();
        let b = AgentId::new();
        gm.create_group("g1", "test", vec![a.clone()]).await;

        assert!(!gm.is_member("g1", &b).await);
        gm.add_member("g1", b.clone()).await.unwrap();
        assert!(gm.is_member("g1", &b).await);

        gm.remove_member("g1", &b).await.unwrap();
        assert!(!gm.is_member("g1", &b).await);
    }

    #[tokio::test]
    async fn send_to_group_records_in_bus() {
        let bus = Arc::new(AgentBus::new(64));
        let gm = GroupManager::new(bus.clone());
        let a = AgentId::new();
        gm.create_group("g1", "test", vec![a.clone()]).await;

        gm.send_to_group(
            "g1",
            a,
            AgentMessage::TechnicalDiscussion {
                topic: "缓存策略".into(),
                position: "用 Redis".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(bus.history_len().await, 1);
    }

    #[tokio::test]
    async fn send_to_nonexistent_group_fails() {
        let gm = make_group_manager();
        let result = gm
            .send_to_group("nope", AgentId::new(), AgentMessage::Shutdown)
            .await;
        assert!(matches!(result, Err(GroupError::NotFound(_))));
    }

    #[tokio::test]
    async fn dissolve_group_removes_it() {
        let gm = make_group_manager();
        gm.create_group("g1", "test", vec![]).await;
        assert_eq!(gm.group_count().await, 1);
        gm.dissolve_group("g1").await.unwrap();
        assert_eq!(gm.group_count().await, 0);
    }

    #[tokio::test]
    async fn dissolve_nonexistent_fails() {
        let gm = make_group_manager();
        let result = gm.dissolve_group("nope").await;
        assert!(matches!(result, Err(GroupError::NotFound(_))));
    }

    #[tokio::test]
    async fn list_groups_works() {
        let gm = make_group_manager();
        gm.create_group("g1", "a", vec![]).await;
        gm.create_group("g2", "b", vec![]).await;
        assert_eq!(gm.list_groups().await.len(), 2);
    }
}
