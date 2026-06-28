//! 团队组建器 — 根据任务需求选择角色并创建 Agent
//!
//! 分析 TaskGraph 中需要的角色，通过 AgentRuntime 创建 Agent。

use std::collections::HashSet;
use std::sync::Arc;

use crate::agent::error::AgentError;
use crate::agent::runtime::AgentRuntime;
use crate::agent::types::{AgentId, Role};

use super::planner::TaskGraph;

/// 团队组建器
pub struct TeamBuilder {
    runtime: Arc<AgentRuntime>,
}

impl TeamBuilder {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }

    /// 分析 TaskGraph，提取所有需要的角色（去重）
    pub fn required_roles(&self, graph: &TaskGraph) -> Vec<Role> {
        let mut seen = HashSet::new();
        let mut roles = Vec::new();
        for task in &graph.tasks {
            if seen.insert(task.assigned_to.clone()) {
                roles.push(task.assigned_to.clone());
            }
        }
        roles
    }

    /// 根据 TaskGraph 组建团队，返回 (Role -> AgentId) 映射
    pub async fn build_team(
        &self,
        graph: &TaskGraph,
    ) -> Result<Vec<(Role, AgentId)>, AgentError> {
        let roles = self.required_roles(graph);
        let mut team = Vec::new();
        for role in roles {
            let id = self
                .runtime
                .spawn(&role, &graph.goal)
                .await?;
            team.push((role, id));
        }
        Ok(team)
    }

    /// 解散团队，销毁所有 Agent
    pub async fn disband_team(&self, team: &[(Role, AgentId)]) -> Result<(), AgentError> {
        for (_, id) in team {
            if let Err(e) = self.runtime.destroy(id).await {
                tracing::debug!("agent destroy skipped (may already be destroyed): {}", e);
            }
        }
        Ok(())
    }

    /// 根据角色查找团队中的 AgentId
    pub fn find_by_role<'a>(team: &'a [(Role, AgentId)], role: &Role) -> Option<&'a AgentId> {
        team.iter()
            .find(|(r, _)| r == role)
            .map(|(_, id)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::identity::vault::IdentityVault;
    use crate::agent::steward::planner::TaskGraphBuilder;
    use crate::agent::types::{Complexity, DeliverableType, Priority};

    fn make_team_builder() -> TeamBuilder {
        let vault = Arc::new(IdentityVault::new());
        let runtime = Arc::new(AgentRuntime::new(vault));
        TeamBuilder::new(runtime)
    }

    fn sample_graph() -> TaskGraph {
        TaskGraphBuilder::new("实现用户认证")
            .add_task(
                "编写PRD",
                "需求文档",
                Role::ProductManager,
                DeliverableType::PRD,
                Priority::P0,
                Complexity::Medium,
            )
            .add_task(
                "架构设计",
                "设计文档",
                Role::Architect,
                DeliverableType::DesignDoc,
                Priority::P0,
                Complexity::High,
            )
            .add_task(
                "编写代码",
                "实现代码",
                Role::Engineer,
                DeliverableType::Code,
                Priority::P0,
                Complexity::High,
            )
            .add_dependency("架构设计", "编写PRD")
            .add_dependency("编写代码", "架构设计")
            .build()
    }

    #[test]
    fn required_roles_deduplicates() {
        let tb = make_team_builder();
        let graph = sample_graph();
        let roles = tb.required_roles(&graph);
        assert_eq!(roles.len(), 3);
    }

    #[tokio::test]
    async fn build_team_creates_agents() {
        let tb = make_team_builder();
        let graph = sample_graph();
        let team = tb.build_team(&graph).await.unwrap();
        assert_eq!(team.len(), 3);
        assert_eq!(tb.runtime.agent_count().await, 3);
    }

    #[tokio::test]
    async fn disband_team_destroys_agents() {
        let tb = make_team_builder();
        let graph = sample_graph();
        let team = tb.build_team(&graph).await.unwrap();
        tb.disband_team(&team).await.unwrap();
        assert_eq!(tb.runtime.agent_count().await, 0);
    }

    #[tokio::test]
    async fn find_by_role_works() {
        let tb = make_team_builder();
        let graph = sample_graph();
        let team = tb.build_team(&graph).await.unwrap();
        assert!(TeamBuilder::find_by_role(&team, &Role::Architect).is_some());
        assert!(TeamBuilder::find_by_role(&team, &Role::Security).is_none());
    }
}
