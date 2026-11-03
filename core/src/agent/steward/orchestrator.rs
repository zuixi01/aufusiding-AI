//! 大管家核心编排逻辑 — 任务拆解、团队组建、分发、监控、解散

use std::sync::Arc;
use std::time::Duration;

use crate::agent::comm::bus::AgentBus;
use crate::agent::comm::protocol::{AgentMessage, EscalationReason, MessageTarget};
use crate::agent::error::AgentError;
use crate::agent::runtime::AgentRuntime;
use crate::agent::types::{AgentId, Role};

use super::escalation::{EscalationAction, EscalationPolicy};
use super::monitor::{ProgressMonitor, TimeoutConfig};
use super::planner::TaskGraph;
use super::team_builder::TeamBuilder;

/// 大管家 — Agent 集群的核心编排器
pub struct Steward {
    id: AgentId,
    bus: Arc<AgentBus>,
    team_builder: TeamBuilder,
    escalation_policy: EscalationPolicy,
}

impl Steward {
    pub fn new(runtime: Arc<AgentRuntime>, bus: Arc<AgentBus>) -> Self {
        let team_builder = TeamBuilder::new(runtime);
        Self {
            id: AgentId::new(),
            bus,
            team_builder,
            escalation_policy: EscalationPolicy::default_policy(),
        }
    }

    pub fn with_escalation_policy(mut self, policy: EscalationPolicy) -> Self {
        self.escalation_policy = policy;
        self
    }

    /// 执行完整编排流程：组建团队 → 分配任务 → 创建监控器
    pub async fn orchestrate(
        &self,
        graph: &mut TaskGraph,
    ) -> Result<OrchestrationSession, AgentError> {
        // 1. 组建团队
        let team = self.team_builder.build_team(graph).await?;
        graph.team = team.iter().map(|(_, id)| id.clone()).collect();

        // 2. 分配 Agent 到子任务
        for task in &mut graph.tasks {
            if let Some(id) = TeamBuilder::find_by_role(&team, &task.assigned_to) {
                task.assigned_agent = Some(id.clone());
            }
        }

        // 3. 创建进度监控器
        let monitor = ProgressMonitor::new(TimeoutConfig::new(Duration::from_secs(300)));
        monitor.init_from_graph(graph).await;

        // 4. 通知就绪任务
        let ready = graph.ready_tasks();
        for task in &ready {
            if let Some(ref agent_id) = task.assigned_agent {
                let envelope = crate::agent::comm::protocol::AgentEnvelope::new(
                    self.id.clone(),
                    MessageTarget::Agent(agent_id.clone()),
                    AgentMessage::TaskAssigned {
                        task_id: task.id.clone(),
                        title: task.title.clone(),
                        description: task.description.clone(),
                    },
                );
                self.bus.send(envelope).await;
            }
        }

        Ok(OrchestrationSession { team, monitor })
    }

    /// 处理升级请求
    pub fn handle_escalation(&self, reason: &EscalationReason) -> EscalationAction {
        self.escalation_policy.should_escalate(reason)
    }

    /// 解散团队
    pub async fn disband(&self, session: &OrchestrationSession) -> Result<(), AgentError> {
        self.team_builder.disband_team(&session.team).await
    }
}

/// 编排会话 — 一次完整编排的运行时状态
pub struct OrchestrationSession {
    pub team: Vec<(Role, AgentId)>,
    pub monitor: ProgressMonitor,
}

impl OrchestrationSession {
    /// 获取团队成员列表
    pub fn team_roles(&self) -> Vec<&Role> {
        self.team.iter().map(|(r, _)| r).collect()
    }

    /// 获取团队大小
    pub fn team_size(&self) -> usize {
        self.team.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::identity::vault::IdentityVault;
    use crate::agent::steward::planner::TaskGraphBuilder;
    use crate::agent::types::{Complexity, DeliverableType, Priority};

    fn make_steward() -> (Steward, Arc<AgentRuntime>) {
        let vault = Arc::new(IdentityVault::new());
        let runtime = Arc::new(AgentRuntime::new(vault));
        let bus = Arc::new(AgentBus::new(64));
        (Steward::new(runtime.clone(), bus), runtime)
    }

    fn sample_graph() -> TaskGraph {
        TaskGraphBuilder::new("实现用户认证")
            .add_task(
                "PRD",
                "需求文档",
                Role::ProductManager,
                DeliverableType::PRD,
                Priority::P0,
                Complexity::Medium,
            )
            .add_task(
                "设计",
                "架构设计",
                Role::Architect,
                DeliverableType::DesignDoc,
                Priority::P0,
                Complexity::High,
            )
            .add_task(
                "编码",
                "实现代码",
                Role::Engineer,
                DeliverableType::Code,
                Priority::P0,
                Complexity::High,
            )
            .add_dependency("设计", "PRD")
            .add_dependency("编码", "设计")
            .build()
    }

    #[tokio::test]
    async fn orchestrate_builds_team_and_assigns() {
        let (steward, runtime) = make_steward();
        let mut graph = sample_graph();
        let session = steward.orchestrate(&mut graph).await.unwrap();

        assert_eq!(session.team_size(), 3);
        assert_eq!(runtime.agent_count().await, 3);

        // 所有任务都应有 assigned_agent
        for task in &graph.tasks {
            assert!(
                task.assigned_agent.is_some(),
                "task {} 未分配 agent",
                task.id
            );
        }
    }

    #[tokio::test]
    async fn orchestrate_sends_ready_task_notifications() {
        let vault = Arc::new(IdentityVault::new());
        let runtime = Arc::new(AgentRuntime::new(vault));
        let bus = Arc::new(AgentBus::new(64));
        let steward = Steward::new(runtime, bus.clone());
        let mut graph = sample_graph();

        steward.orchestrate(&mut graph).await.unwrap();

        // 只有 PRD 是就绪任务，应发送 1 条 TaskAssigned 消息
        assert_eq!(bus.history_len().await, 1);
    }

    #[tokio::test]
    async fn disband_destroys_all_agents() {
        let (steward, runtime) = make_steward();
        let mut graph = sample_graph();
        let session = steward.orchestrate(&mut graph).await.unwrap();
        assert_eq!(runtime.agent_count().await, 3);

        steward.disband(&session).await.unwrap();
        assert_eq!(runtime.agent_count().await, 0);
    }

    #[test]
    fn handle_escalation_delegates_to_policy() {
        let (steward, _) = make_steward();
        assert_eq!(
            steward.handle_escalation(&EscalationReason::SecurityConcern),
            EscalationAction::Roundtable
        );
        assert_eq!(
            steward.handle_escalation(&EscalationReason::NeedHumanDecision),
            EscalationAction::EscalateToUser
        );
    }
}
