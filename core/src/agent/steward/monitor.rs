//! 进度监控 — 追踪 Agent 状态和任务进度，检测超时

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::agent::types::AgentId;

use super::planner::{SubTaskStatus, TaskGraph, TaskId};

/// 任务进度快照
#[derive(Debug, Clone)]
pub struct TaskProgress {
    pub task_id: TaskId,
    pub agent_id: Option<AgentId>,
    pub status: SubTaskStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub progress_pct: f64,
}

/// 超时配置
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub default_timeout: Duration,
    pub timeout_overrides: HashMap<TaskId, Duration>,
}

impl TimeoutConfig {
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            default_timeout,
            timeout_overrides: HashMap::new(),
        }
    }

    pub fn with_override(mut self, task_id: impl Into<String>, timeout: Duration) -> Self {
        self.timeout_overrides.insert(task_id.into(), timeout);
        self
    }

    pub fn timeout_for(&self, task_id: &str) -> Duration {
        self.timeout_overrides
            .get(task_id)
            .copied()
            .unwrap_or(self.default_timeout)
    }
}

/// 进度监控器
pub struct ProgressMonitor {
    progress: Arc<RwLock<HashMap<TaskId, TaskProgress>>>,
    config: TimeoutConfig,
}

impl ProgressMonitor {
    pub fn new(config: TimeoutConfig) -> Self {
        Self {
            progress: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// 从 TaskGraph 初始化进度追踪
    pub async fn init_from_graph(&self, graph: &TaskGraph) {
        let mut progress = self.progress.write().await;
        for task in &graph.tasks {
            progress.insert(
                task.id.clone(),
                TaskProgress {
                    task_id: task.id.clone(),
                    agent_id: task.assigned_agent.clone(),
                    status: task.status.clone(),
                    started_at: None,
                    progress_pct: 0.0,
                },
            );
        }
    }

    /// 标记任务开始
    pub async fn mark_started(&self, task_id: &str, agent_id: AgentId) {
        let mut progress = self.progress.write().await;
        if let Some(p) = progress.get_mut(task_id) {
            p.status = SubTaskStatus::InProgress;
            p.started_at = Some(Utc::now());
            p.agent_id = Some(agent_id);
        }
    }

    /// 更新进度百分比
    pub async fn update_progress(&self, task_id: &str, pct: f64) {
        let mut progress = self.progress.write().await;
        if let Some(p) = progress.get_mut(task_id) {
            p.progress_pct = pct.clamp(0.0, 100.0);
        }
    }

    /// 标记任务完成
    pub async fn mark_completed(&self, task_id: &str) {
        let mut progress = self.progress.write().await;
        if let Some(p) = progress.get_mut(task_id) {
            p.status = SubTaskStatus::Completed;
            p.progress_pct = 100.0;
        }
    }

    /// 检测超时任务
    pub async fn detect_timeouts(&self) -> Vec<TaskId> {
        let progress = self.progress.read().await;
        let now = Utc::now();
        progress
            .values()
            .filter(|p| {
                p.status == SubTaskStatus::InProgress
                    && p.started_at
                        .map(|s| {
                            let elapsed = (now - s).to_std().unwrap_or(Duration::ZERO);
                            elapsed > self.config.timeout_for(&p.task_id)
                        })
                        .unwrap_or(false)
            })
            .map(|p| p.task_id.clone())
            .collect()
    }

    /// 获取所有任务进度
    pub async fn all_progress(&self) -> Vec<TaskProgress> {
        self.progress.read().await.values().cloned().collect()
    }

    /// 获取单个任务进度
    pub async fn get_progress(&self, task_id: &str) -> Option<TaskProgress> {
        self.progress.read().await.get(task_id).cloned()
    }

    /// 总体完成百分比
    pub async fn overall_progress(&self) -> f64 {
        let progress = self.progress.read().await;
        if progress.is_empty() {
            return 0.0;
        }
        let total: f64 = progress.values().map(|p| p.progress_pct).sum();
        total / progress.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::steward::planner::TaskGraphBuilder;
    use crate::agent::types::{Complexity, DeliverableType, Priority, Role};

    fn make_monitor() -> ProgressMonitor {
        ProgressMonitor::new(TimeoutConfig::new(Duration::from_secs(300)))
    }

    fn simple_graph() -> TaskGraph {
        TaskGraphBuilder::new("test")
            .add_task("T1", "d1", Role::Engineer, DeliverableType::Code, Priority::P0, Complexity::Low)
            .add_task("T2", "d2", Role::QA, DeliverableType::TestSuite, Priority::P1, Complexity::Low)
            .add_dependency("T2", "T1")
            .build()
    }

    #[tokio::test]
    async fn init_from_graph_tracks_all_tasks() {
        let mon = make_monitor();
        let g = simple_graph();
        mon.init_from_graph(&g).await;
        assert_eq!(mon.all_progress().await.len(), 2);
    }

    #[tokio::test]
    async fn mark_started_and_completed() {
        let mon = make_monitor();
        let g = simple_graph();
        mon.init_from_graph(&g).await;

        let agent = AgentId::new();
        mon.mark_started("T1", agent).await;
        let p = mon.get_progress("T1").await.unwrap();
        assert_eq!(p.status, SubTaskStatus::InProgress);
        assert!(p.started_at.is_some());

        mon.mark_completed("T1").await;
        let p = mon.get_progress("T1").await.unwrap();
        assert_eq!(p.status, SubTaskStatus::Completed);
        assert!((p.progress_pct - 100.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn update_progress_clamps() {
        let mon = make_monitor();
        let g = simple_graph();
        mon.init_from_graph(&g).await;
        mon.update_progress("T1", 150.0).await;
        let p = mon.get_progress("T1").await.unwrap();
        assert!((p.progress_pct - 100.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn overall_progress_calculation() {
        let mon = make_monitor();
        let g = simple_graph();
        mon.init_from_graph(&g).await;
        mon.mark_completed("T1").await;
        let overall = mon.overall_progress().await;
        assert!((overall - 50.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn detect_timeouts_with_short_timeout() {
        let mon = ProgressMonitor::new(TimeoutConfig::new(Duration::from_millis(1)));
        let g = simple_graph();
        mon.init_from_graph(&g).await;
        mon.mark_started("T1", AgentId::new()).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        let timeouts = mon.detect_timeouts().await;
        assert!(timeouts.contains(&"T1".to_string()));
    }

    #[tokio::test]
    async fn no_timeouts_when_within_limit() {
        let mon = make_monitor(); // 300s timeout
        let g = simple_graph();
        mon.init_from_graph(&g).await;
        mon.mark_started("T1", AgentId::new()).await;
        let timeouts = mon.detect_timeouts().await;
        assert!(timeouts.is_empty());
    }

    #[tokio::test]
    async fn timeout_override_per_task() {
        let config = TimeoutConfig::new(Duration::from_secs(300))
            .with_override("T1", Duration::from_millis(1));
        let mon = ProgressMonitor::new(config);
        let g = simple_graph();
        mon.init_from_graph(&g).await;
        mon.mark_started("T1", AgentId::new()).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        let timeouts = mon.detect_timeouts().await;
        assert!(timeouts.contains(&"T1".to_string()));
    }
}
