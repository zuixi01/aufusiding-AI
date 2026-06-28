//! Agent 运行时 — 管理 Agent 的创建、销毁和生命周期

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;

use crate::agent::error::AgentError;
use crate::agent::identity::injection::{AgentIdentity, IdentityInjector};
use crate::agent::identity::vault::IdentityVault;
use crate::agent::types::{AgentId, AgentState, Deliverable, Role};

/// 运行中的 Agent 实例
#[derive(Debug, Clone)]
pub struct LiveAgent {
    pub id: AgentId,
    pub identity: AgentIdentity,
    pub state: AgentState,
    pub deliverables: Vec<Deliverable>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Agent 运行时 — 管理 Agent 集群的生命周期
pub struct AgentRuntime {
    agents: Arc<RwLock<HashMap<AgentId, LiveAgent>>>,
    injector: Arc<IdentityInjector>,
}

impl AgentRuntime {
    pub fn new(vault: Arc<IdentityVault>) -> Self {
        let injector = Arc::new(IdentityInjector::new(vault));
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            injector,
        }
    }

    /// 创建 Agent，注入身份，返回 AgentId
    pub async fn spawn(
        &self,
        role: &Role,
        task_context: &str,
    ) -> Result<AgentId, AgentError> {
        let identity = self.injector.inject(role, task_context)?;
        let id = AgentId::new();
        let agent = LiveAgent {
            id: id.clone(),
            identity,
            state: AgentState::Idle,
            deliverables: vec![],
            created_at: Utc::now(),
        };
        let mut agents = self.agents.write().await;
        agents.insert(id.clone(), agent);
        Ok(id)
    }

    /// 销毁指定 Agent
    pub async fn destroy(&self, id: &AgentId) -> Result<(), AgentError> {
        let mut agents = self.agents.write().await;
        agents
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| AgentError::NotFound(id.clone()))
    }

    /// 销毁所有 Agent
    pub async fn destroy_all(&self) -> Result<(), AgentError> {
        let mut agents = self.agents.write().await;
        agents.clear();
        Ok(())
    }

    /// 获取 Agent 快照（clone）
    pub async fn get_agent(&self, id: &AgentId) -> Option<LiveAgent> {
        let agents = self.agents.read().await;
        agents.get(id).cloned()
    }

    /// 列出所有 Agent 的摘要信息
    pub async fn list_agents(&self) -> Vec<(AgentId, Role, AgentState)> {
        let agents = self.agents.read().await;
        agents
            .values()
            .map(|a| (a.id.clone(), a.identity.role.clone(), a.state.clone()))
            .collect()
    }

    /// 更新 Agent 状态
    pub async fn update_state(
        &self,
        id: &AgentId,
        state: AgentState,
    ) -> Result<(), AgentError> {
        let mut agents = self.agents.write().await;
        let agent = agents
            .get_mut(id)
            .ok_or_else(|| AgentError::NotFound(id.clone()))?;
        agent.state = state;
        Ok(())
    }

    /// 添加交付物
    pub async fn add_deliverable(
        &self,
        id: &AgentId,
        deliverable: Deliverable,
    ) -> Result<(), AgentError> {
        let mut agents = self.agents.write().await;
        let agent = agents
            .get_mut(id)
            .ok_or_else(|| AgentError::NotFound(id.clone()))?;
        agent.deliverables.push(deliverable);
        Ok(())
    }

    /// 当前 Agent 数量
    pub async fn agent_count(&self) -> usize {
        let agents = self.agents.read().await;
        agents.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::DeliverableType;

    fn test_runtime() -> AgentRuntime {
        let vault = Arc::new(IdentityVault::new());
        AgentRuntime::new(vault)
    }

    #[tokio::test]
    async fn spawn_creates_agent_with_identity() {
        let rt = test_runtime();
        let id = rt.spawn(&Role::Architect, "设计缓存").await.unwrap();
        let agent = rt.get_agent(&id).await.unwrap();
        assert_eq!(agent.identity.role, Role::Architect);
        assert_eq!(agent.state, AgentState::Idle);
        assert!(agent.persona_prompt_contains("缓存"));
    }

    #[tokio::test]
    async fn spawn_all_eight_roles() {
        let rt = test_runtime();
        let roles = [
            Role::Architect, Role::Engineer, Role::Security, Role::QA,
            Role::ProductManager, Role::Researcher, Role::DevOps, Role::DataAnalyst,
        ];
        for role in &roles {
            rt.spawn(role, "test").await.unwrap();
        }
        assert_eq!(rt.agent_count().await, 8);
    }

    #[tokio::test]
    async fn destroy_removes_agent() {
        let rt = test_runtime();
        let id = rt.spawn(&Role::Engineer, "task").await.unwrap();
        assert_eq!(rt.agent_count().await, 1);
        rt.destroy(&id).await.unwrap();
        assert_eq!(rt.agent_count().await, 0);
        assert!(rt.get_agent(&id).await.is_none());
    }

    #[tokio::test]
    async fn destroy_nonexistent_returns_not_found() {
        let rt = test_runtime();
        let fake_id = AgentId::new();
        let result = rt.destroy(&fake_id).await;
        assert!(matches!(result, Err(AgentError::NotFound(_))));
    }

    #[tokio::test]
    async fn destroy_all_clears_registry() {
        let rt = test_runtime();
        rt.spawn(&Role::Architect, "a").await.unwrap();
        rt.spawn(&Role::Engineer, "b").await.unwrap();
        rt.spawn(&Role::QA, "c").await.unwrap();
        assert_eq!(rt.agent_count().await, 3);
        rt.destroy_all().await.unwrap();
        assert_eq!(rt.agent_count().await, 0);
    }

    #[tokio::test]
    async fn get_agent_returns_none_for_missing() {
        let rt = test_runtime();
        assert!(rt.get_agent(&AgentId::new()).await.is_none());
    }

    #[tokio::test]
    async fn list_agents_returns_all() {
        let rt = test_runtime();
        rt.spawn(&Role::Architect, "a").await.unwrap();
        rt.spawn(&Role::Security, "b").await.unwrap();
        let list = rt.list_agents().await;
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn update_state_transitions() {
        let rt = test_runtime();
        let id = rt.spawn(&Role::Engineer, "task").await.unwrap();
        let new_state = AgentState::Working {
            task_id: "t-1".into(),
        };
        rt.update_state(&id, new_state.clone()).await.unwrap();
        let agent = rt.get_agent(&id).await.unwrap();
        assert_eq!(agent.state, new_state);
    }

    #[tokio::test]
    async fn update_state_nonexistent_fails() {
        let rt = test_runtime();
        let result = rt
            .update_state(&AgentId::new(), AgentState::Idle)
            .await;
        assert!(matches!(result, Err(AgentError::NotFound(_))));
    }

    #[tokio::test]
    async fn add_deliverable_works() {
        let rt = test_runtime();
        let id = rt.spawn(&Role::Engineer, "task").await.unwrap();
        let deliverable = Deliverable {
            id: "d-1".into(),
            title: "Test".into(),
            content: "content".into(),
            deliverable_type: DeliverableType::Code,
            created_by: id.clone(),
            created_at: Utc::now(),
        };
        rt.add_deliverable(&id, deliverable).await.unwrap();
        let agent = rt.get_agent(&id).await.unwrap();
        assert_eq!(agent.deliverables.len(), 1);
    }

    #[tokio::test]
    async fn spawn_initial_state_is_idle() {
        let rt = test_runtime();
        let id = rt.spawn(&Role::QA, "test").await.unwrap();
        let agent = rt.get_agent(&id).await.unwrap();
        assert_eq!(agent.state, AgentState::Idle);
    }

    #[tokio::test]
    async fn concurrent_spawn_and_destroy() {
        let rt = Arc::new(test_runtime());
        let mut handles = vec![];

        // Spawn 10 agents concurrently
        for i in 0..10 {
            let rt = rt.clone();
            handles.push(tokio::spawn(async move {
                rt.spawn(&Role::Engineer, &format!("task-{}", i)).await
            }));
        }

        let mut ids = vec![];
        for h in handles {
            ids.push(h.await.unwrap().unwrap());
        }
        assert_eq!(rt.agent_count().await, 10);

        // Destroy all concurrently
        let mut handles = vec![];
        for id in ids {
            let rt = rt.clone();
            handles.push(tokio::spawn(async move { rt.destroy(&id).await }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        assert_eq!(rt.agent_count().await, 0);
    }

    // Helper trait for test assertions
    impl LiveAgent {
        fn persona_prompt_contains(&self, s: &str) -> bool {
            self.identity.persona_prompt.contains(s)
        }
    }
}
