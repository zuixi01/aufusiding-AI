//! 任务拆解器 — 将用户请求拆解为 TaskGraph DAG
//!
//! 使用 petgraph 构建有向无环图，管理子任务依赖关系。

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::Topo;
use uuid::Uuid;

use crate::agent::types::{AgentId, Complexity, DeliverableType, Priority, Role};

/// TaskGraph ID
pub type TaskGraphId = String;

/// 子任务 ID
pub type TaskId = String;

/// 期望交付物规格
#[derive(Debug, Clone)]
pub struct DeliverableSpec {
    pub deliverable_type: DeliverableType,
    pub description: String,
}

/// 子任务
#[derive(Debug, Clone)]
pub struct SubTask {
    pub id: TaskId,
    pub title: String,
    pub description: String,
    pub assigned_to: Role,
    pub assigned_agent: Option<AgentId>,
    pub deliverable_spec: DeliverableSpec,
    pub acceptance_criteria: Vec<String>,
    pub priority: Priority,
    pub estimated_complexity: Complexity,
    pub status: SubTaskStatus,
    pub created_at: DateTime<Utc>,
}

/// 子任务状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubTaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Failed,
}

/// 任务图 — 有向无环图表示子任务及其依赖
#[derive(Debug, Clone)]
pub struct TaskGraph {
    pub id: TaskGraphId,
    pub goal: String,
    pub tasks: Vec<SubTask>,
    pub team: Vec<AgentId>,
    graph: DiGraph<TaskId, ()>,
    node_map: HashMap<TaskId, NodeIndex>,
}

impl TaskGraph {
    /// 获取可以立即执行的任务（无未完成依赖）
    pub fn ready_tasks(&self) -> Vec<&SubTask> {
        self.tasks
            .iter()
            .filter(|t| {
                t.status == SubTaskStatus::Pending && self.dependencies_met(&t.id)
            })
            .collect()
    }

    /// 检查指定任务的所有依赖是否已完成
    pub fn dependencies_met(&self, task_id: &str) -> bool {
        let Some(&node_idx) = self.node_map.get(task_id) else {
            return false;
        };
        // 检查所有入边的源节点对应的任务是否已完成
        self.graph
            .neighbors_directed(node_idx, petgraph::Direction::Incoming)
            .all(|dep_idx| {
                let dep_id = &self.graph[dep_idx];
                self.tasks
                    .iter()
                    .find(|t| &t.id == dep_id)
                    .map(|t| t.status == SubTaskStatus::Completed)
                    .unwrap_or(false)
            })
    }

    /// 获取拓扑排序后的任务 ID 列表
    pub fn topological_order(&self) -> Vec<TaskId> {
        let mut topo = Topo::new(&self.graph);
        let mut order = Vec::new();
        while let Some(idx) = topo.next(&self.graph) {
            order.push(self.graph[idx].clone());
        }
        order
    }

    /// 标记任务完成
    pub fn complete_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = SubTaskStatus::Completed;
            true
        } else {
            false
        }
    }

    /// 标记任务进行中
    pub fn start_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = SubTaskStatus::InProgress;
            true
        } else {
            false
        }
    }

    /// 标记任务失败
    pub fn fail_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = SubTaskStatus::Failed;
            true
        } else {
            false
        }
    }

    /// 分配 Agent 到任务
    pub fn assign_agent(&mut self, task_id: &str, agent_id: AgentId) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.assigned_agent = Some(agent_id);
            true
        } else {
            false
        }
    }

    /// 所有任务是否都已完成
    pub fn is_complete(&self) -> bool {
        self.tasks.iter().all(|t| t.status == SubTaskStatus::Completed)
    }

    /// 获取指定任务
    pub fn get_task(&self, task_id: &str) -> Option<&SubTask> {
        self.tasks.iter().find(|t| t.id == task_id)
    }

    /// 任务总数
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    /// 检查是否有环（应始终为 false）
    pub fn has_cycle(&self) -> bool {
        petgraph::algo::is_cyclic_directed(&self.graph)
    }
}

// ─── TaskGraphBuilder ─────────────────────────────────────

/// TaskGraph 构建器 — 流式 API 构建任务图
pub struct TaskGraphBuilder {
    goal: String,
    tasks: Vec<SubTask>,
    dependencies: Vec<(String, String)>, // (task_title, depends_on_title)
}

impl TaskGraphBuilder {
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            tasks: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    pub fn add_task(
        mut self,
        title: impl Into<String>,
        description: impl Into<String>,
        role: Role,
        deliverable_type: DeliverableType,
        priority: Priority,
        complexity: Complexity,
    ) -> Self {
        let title = title.into();
        let task = SubTask {
            id: title.clone(),
            title: title.clone(),
            description: description.into(),
            assigned_to: role,
            assigned_agent: None,
            deliverable_spec: DeliverableSpec {
                deliverable_type,
                description: format!("{} 交付物", title),
            },
            acceptance_criteria: vec![],
            priority,
            estimated_complexity: complexity,
            status: SubTaskStatus::Pending,
            created_at: Utc::now(),
        };
        self.tasks.push(task);
        self
    }

    pub fn add_acceptance_criteria(
        mut self,
        task_title: &str,
        criteria: impl Into<String>,
    ) -> Self {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.title == task_title) {
            task.acceptance_criteria.push(criteria.into());
        }
        self
    }

    /// 添加依赖: task_title 依赖 depends_on_title
    pub fn add_dependency(
        mut self,
        task_title: impl Into<String>,
        depends_on_title: impl Into<String>,
    ) -> Self {
        self.dependencies
            .push((task_title.into(), depends_on_title.into()));
        self
    }

    pub fn build(mut self) -> TaskGraph {
        // 任务去重：合并语义重复的子任务
        let dedup = super::dedup::TaskDeduplicator::new(0.7);
        let duplicates = dedup.find_duplicates(&self.tasks);
        if !duplicates.is_empty() {
            dedup.merge(&mut self.tasks, &duplicates);
            // 清理引用已移除任务的依赖
            let task_ids: std::collections::HashSet<String> =
                self.tasks.iter().map(|t| t.id.clone()).collect();
            self.dependencies
                .retain(|(a, b)| task_ids.contains(a) && task_ids.contains(b));
        }

        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        // 添加节点
        for task in &self.tasks {
            let idx = graph.add_node(task.id.clone());
            node_map.insert(task.id.clone(), idx);
        }

        // 添加边（依赖方向：depends_on -> task）
        for (task_title, dep_title) in &self.dependencies {
            if let (Some(&task_idx), Some(&dep_idx)) =
                (node_map.get(task_title), node_map.get(dep_title))
            {
                graph.add_edge(dep_idx, task_idx, ());
            }
        }

        TaskGraph {
            id: Uuid::new_v4().to_string(),
            goal: self.goal,
            tasks: self.tasks,
            team: vec![],
            graph,
            node_map,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> TaskGraph {
        TaskGraphBuilder::new("实现用户认证")
            .add_task("PRD", "需求文档", Role::ProductManager, DeliverableType::PRD, Priority::P0, Complexity::Medium)
            .add_task("设计", "架构设计", Role::Architect, DeliverableType::DesignDoc, Priority::P0, Complexity::High)
            .add_task("编码", "实现代码", Role::Engineer, DeliverableType::Code, Priority::P0, Complexity::High)
            .add_task("测试", "测试套件", Role::QA, DeliverableType::TestSuite, Priority::P1, Complexity::Medium)
            .add_dependency("设计", "PRD")
            .add_dependency("编码", "设计")
            .add_dependency("测试", "PRD")
            .add_dependency("测试", "编码")
            .build()
    }

    #[test]
    fn build_graph_has_correct_task_count() {
        let g = sample_graph();
        assert_eq!(g.task_count(), 4);
    }

    #[test]
    fn no_cycle_in_dag() {
        let g = sample_graph();
        assert!(!g.has_cycle());
    }

    #[test]
    fn topological_order_respects_dependencies() {
        let g = sample_graph();
        let order = g.topological_order();
        let pos = |name: &str| order.iter().position(|t| t == name).unwrap();
        assert!(pos("PRD") < pos("设计"));
        assert!(pos("设计") < pos("编码"));
        assert!(pos("PRD") < pos("测试"));
        assert!(pos("编码") < pos("测试"));
    }

    #[test]
    fn ready_tasks_initially_only_root() {
        let g = sample_graph();
        let ready = g.ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "PRD");
    }

    #[test]
    fn complete_task_unlocks_dependents() {
        let mut g = sample_graph();
        g.complete_task("PRD");
        let ready = g.ready_tasks();
        // PRD完成后，设计 和 测试(还依赖编码) 中只有设计可以开始
        let ready_ids: Vec<&str> = ready.iter().map(|t| t.id.as_str()).collect();
        assert!(ready_ids.contains(&"设计"));
        assert!(!ready_ids.contains(&"测试")); // 测试还依赖编码
    }

    #[test]
    fn complete_all_tasks() {
        let mut g = sample_graph();
        assert!(!g.is_complete());
        g.complete_task("PRD");
        g.complete_task("设计");
        g.complete_task("编码");
        g.complete_task("测试");
        assert!(g.is_complete());
    }

    #[test]
    fn start_and_fail_task() {
        let mut g = sample_graph();
        g.start_task("PRD");
        assert_eq!(g.get_task("PRD").unwrap().status, SubTaskStatus::InProgress);
        g.fail_task("PRD");
        assert_eq!(g.get_task("PRD").unwrap().status, SubTaskStatus::Failed);
    }

    #[test]
    fn assign_agent_to_task() {
        let mut g = sample_graph();
        let agent = AgentId::new();
        assert!(g.assign_agent("PRD", agent.clone()));
        assert_eq!(g.get_task("PRD").unwrap().assigned_agent, Some(agent));
    }

    #[test]
    fn get_nonexistent_task() {
        let g = sample_graph();
        assert!(g.get_task("不存在").is_none());
    }

    #[test]
    fn acceptance_criteria_builder() {
        let g = TaskGraphBuilder::new("test")
            .add_task("T1", "desc", Role::Engineer, DeliverableType::Code, Priority::P0, Complexity::Low)
            .add_acceptance_criteria("T1", "测试覆盖率≥80%")
            .add_acceptance_criteria("T1", "无Critical漏洞")
            .build();
        let t = g.get_task("T1").unwrap();
        assert_eq!(t.acceptance_criteria.len(), 2);
    }
}
