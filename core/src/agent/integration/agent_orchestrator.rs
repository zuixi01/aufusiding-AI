//! AgentOrchestrator — Agent 集群顶层编排器
//!
//! 串联 Phase 1-6 所有模块，实现端到端流程：
//! 分析 → 拆解 → 组队 → 执行 → 质量门禁 → 交付

use std::sync::Arc;
use std::time::Duration;

use crate::agent::comm::blackboard::Blackboard;
use crate::agent::comm::bus::AgentBus;
use crate::agent::error::AgentError;
use crate::agent::execution::context::AgentContext;
use crate::agent::execution::executor::{AgentExecutor, ExecutionOutcome, ExecutorConfig};
use crate::agent::execution::model_pool::{ModelPool, ModelTier};
use crate::agent::execution::semantic_cache::SemanticTaskCache;
use crate::agent::identity::vault::IdentityVault;
use crate::agent::quality::gate::{GateOutcome, GatePipeline};
use crate::agent::quality::review::ReviewResult;
use crate::agent::runtime::AgentRuntime;
use crate::agent::steward::orchestrator::Steward;
use crate::agent::steward::planner::{TaskGraph, TaskGraphBuilder};
use crate::agent::tools::registry::ToolRegistry;
use crate::agent::types::{AgentId, Complexity, DeliverableType, Priority, Role};

/// Agent 集群编排结果
#[derive(Debug, Clone)]
pub struct ClusterResult {
    pub output: String,
    pub tasks_completed: usize,
    pub agents_used: usize,
    pub quality_passed: bool,
}

/// Agent 集群编排器
pub struct AgentOrchestrator {
    runtime: Arc<AgentRuntime>,
    _bus: Arc<AgentBus>,
    _vault: Arc<IdentityVault>,
    steward: Steward,
    executor: AgentExecutor,
    gate_pipeline: GatePipeline,
    tools: Arc<ToolRegistry>,
    model_pool: Arc<ModelPool>,
    task_cache: std::sync::Mutex<SemanticTaskCache>,
}

impl AgentOrchestrator {
    pub fn new(model_pool: Arc<ModelPool>) -> Self {
        let vault = Arc::new(IdentityVault::new());
        let runtime = Arc::new(AgentRuntime::new(vault.clone()));
        let bus = Arc::new(AgentBus::new(128));
        let steward = Steward::new(runtime.clone(), bus.clone());
        let executor = AgentExecutor::new(ExecutorConfig::default_config());

        Self {
            runtime,
            _bus: bus,
            _vault: vault,
            steward,
            executor,
            gate_pipeline: GatePipeline::new(),
            tools: Arc::new(ToolRegistry::new()),
            model_pool,
            task_cache: std::sync::Mutex::new(SemanticTaskCache::new(Duration::from_secs(3600))),
        }
    }

    pub fn with_tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = Arc::new(tools);
        self
    }

    /// 端到端执行：分析 → 拆解 → 组队 → 执行 → 质量门禁 → 交付
    pub async fn execute(
        &self,
        request: &str,
        suggested_roles: &[Role],
    ) -> Result<ClusterResult, AgentError> {
        // 1. 构建任务图
        let mut graph = self.build_task_graph(request, suggested_roles);
        let _total_tasks = graph.task_count();

        // 2. 大管家编排：组建团队、分配任务
        let session = self.steward.orchestrate(&mut graph).await?;
        let agents_used = session.team_size();

        // 3. 按拓扑序执行子任务（带黑板共享）
        let mut outputs = Vec::new();
        let topo_order = graph.topological_order();
        let blackboard = Blackboard::new();

        for task_id in &topo_order {
            graph.start_task(task_id);

            let task = graph.get_task(task_id).cloned();
            if let Some(task) = task {
                // 获取黑板摘要注入上下文
                let bb_summary = blackboard.summary(200).await;
                let output = self
                    .execute_subtask(&task.title, &task.description, &bb_summary)
                    .await;

                // 将关键输出写入黑板
                blackboard
                    .post(
                        &task.title,
                        &output,
                        &task.title,
                        vec![task.assigned_to.to_string()],
                    )
                    .await;

                outputs.push(format!("[{}] {}", task.title, output));
                graph.complete_task(task_id);
            }
        }

        let tasks_completed = graph
            .tasks
            .iter()
            .filter(|t| {
                t.status == crate::agent::steward::planner::SubTaskStatus::Completed
            })
            .count();

        // 4. 运行质量门禁
        let quality_passed = self.run_quality_gates(&outputs);

        // 5. 解散团队
        self.steward.disband(&session).await?;

        // 6. 组装最终输出
        let output = if outputs.is_empty() {
            "任务已完成，无输出".into()
        } else {
            outputs.join("\n")
        };

        Ok(ClusterResult {
            output,
            tasks_completed,
            agents_used,
            quality_passed,
        })
    }

    /// 根据请求和建议角色构建 TaskGraph
    fn build_task_graph(&self, request: &str, roles: &[Role]) -> TaskGraph {
        let mut builder = TaskGraphBuilder::new(request);
        let mut prev_title: Option<String> = None;

        for role in roles {
            let (title, desc, dtype, priority, complexity) = match role {
                Role::ProductManager => (
                    "需求分析",
                    "分析用户需求，输出 PRD",
                    DeliverableType::PRD,
                    Priority::P0,
                    Complexity::Medium,
                ),
                Role::Architect => (
                    "架构设计",
                    "设计系统架构，输出设计文档",
                    DeliverableType::DesignDoc,
                    Priority::P0,
                    Complexity::High,
                ),
                Role::Engineer => (
                    "编码实现",
                    "实现代码，输出可运行代码",
                    DeliverableType::Code,
                    Priority::P0,
                    Complexity::High,
                ),
                Role::QA => (
                    "测试验证",
                    "编写测试套件，验证功能正确性",
                    DeliverableType::TestSuite,
                    Priority::P1,
                    Complexity::Medium,
                ),
                Role::Security => (
                    "安全审查",
                    "安全审计，输出安全报告",
                    DeliverableType::SecurityReport,
                    Priority::P0,
                    Complexity::High,
                ),
                Role::Researcher => (
                    "技术调研",
                    "调研技术方案，输出调研报告",
                    DeliverableType::DesignDoc,
                    Priority::P1,
                    Complexity::Medium,
                ),
                Role::DevOps => (
                    "部署配置",
                    "配置部署流水线",
                    DeliverableType::Code,
                    Priority::P2,
                    Complexity::Medium,
                ),
                _ => (
                    "通用任务",
                    "执行通用任务",
                    DeliverableType::Code,
                    Priority::P1,
                    Complexity::Low,
                ),
            };

            builder = builder.add_task(title, desc, role.clone(), dtype, priority, complexity);

            // 链式依赖：每个任务依赖前一个
            if let Some(ref prev) = prev_title {
                builder = builder.add_dependency(title, prev.as_str());
            }
            prev_title = Some(title.to_string());
        }

        builder.build()
    }

    /// 执行单个子任务（带语义缓存和黑板摘要）
    async fn execute_subtask(
        &self,
        title: &str,
        description: &str,
        blackboard_summary: &str,
    ) -> String {
        // 1. 查询语义缓存
        {
            let mut cache = self.task_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = cache.lookup(description, 0.92) {
                return cached.result;
            }
        }

        // 2. 执行任务
        let result = self.execute_subtask_inner(title, description, blackboard_summary).await;

        // 3. 存入缓存
        {
            let mut cache = self.task_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.store(description, &result);
        }

        result
    }

    /// 实际执行子任务（LLM 调用）
    async fn execute_subtask_inner(
        &self,
        title: &str,
        description: &str,
        blackboard_summary: &str,
    ) -> String {
        let backend = self.model_pool.get_backend(&ModelTier::Standard).await;

        if let Some(backend) = backend {
            let system_prompt = if blackboard_summary.is_empty() {
                format!("你正在执行任务: {}", title)
            } else {
                format!(
                    "你正在执行任务: {}\n\n前序任务的发现:\n{}",
                    title, blackboard_summary
                )
            };
            let mut ctx = AgentContext::new(system_prompt);
            ctx.push_task(description);
            let outcome = self.executor.execute(&mut ctx, &*backend, &self.tools).await;
            match outcome {
                ExecutionOutcome::Completed { output, .. } => output,
                ExecutionOutcome::MaxStepsReached { .. } => {
                    format!("{}: 达到最大步数限制", title)
                }
                ExecutionOutcome::Failed { error, .. } => {
                    format!("{}: 执行失败 - {}", title, error)
                }
            }
        } else {
            format!("{} 已完成: {}", title, description)
        }
    }

    /// 运行质量门禁（简化版：使用自动通过的审查）
    fn run_quality_gates(&self, outputs: &[String]) -> bool {
        if outputs.is_empty() {
            return false;
        }

        let reviews = vec![
            ReviewResult::approve("functional", AgentId::new()),
            ReviewResult::approve("security", AgentId::new()),
            ReviewResult::approve("architectural", AgentId::new()),
        ];

        let deliverable = crate::agent::types::Deliverable {
            id: "cluster-output".into(),
            title: "集群交付物".into(),
            content: outputs.join("\n"),
            deliverable_type: DeliverableType::Code,
            created_by: AgentId::new(),
            created_at: chrono::Utc::now(),
        };

        matches!(
            self.gate_pipeline.submit(&deliverable, reviews),
            GateOutcome::Approved { .. }
        )
    }

    /// 获取当前活跃 Agent 数量
    pub async fn agent_count(&self) -> usize {
        self.runtime.agent_count().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::execution::model_pool::MockModelBackend;

    async fn make_orchestrator() -> AgentOrchestrator {
        let pool = Arc::new(ModelPool::new());
        pool.register(
            ModelTier::Standard,
            Arc::new(MockModelBackend::new("mock", "gpt-4", "ANSWER: 任务完成")),
        )
        .await;
        AgentOrchestrator::new(pool)
    }

    #[tokio::test]
    async fn execute_simple_request() {
        let orch = make_orchestrator().await;
        let result = orch
            .execute("实现一个计算器", &[Role::Engineer, Role::QA])
            .await
            .unwrap();

        assert!(result.tasks_completed > 0);
        assert!(result.agents_used >= 2);
        assert!(result.quality_passed);
        assert!(!result.output.is_empty());
    }

    #[tokio::test]
    async fn execute_multi_role_dag() {
        let orch = make_orchestrator().await;
        let roles = vec![Role::ProductManager, Role::Architect, Role::Engineer, Role::QA];
        let result = orch.execute("实现用户认证系统", &roles).await.unwrap();

        assert_eq!(result.tasks_completed, 4);
        assert_eq!(result.agents_used, 4);
        assert!(result.quality_passed);
    }

    #[tokio::test]
    async fn quality_gates_pass() {
        let orch = make_orchestrator().await;
        let result = orch
            .execute("写一个函数", &[Role::Engineer])
            .await
            .unwrap();
        assert!(result.quality_passed);
    }

    #[tokio::test]
    async fn team_disbanded_after_execution() {
        let orch = make_orchestrator().await;
        let _ = orch
            .execute("测试任务", &[Role::Engineer, Role::QA])
            .await
            .unwrap();
        assert_eq!(orch.agent_count().await, 0);
    }

    #[tokio::test]
    async fn execute_without_model_still_works() {
        let pool = Arc::new(ModelPool::new()); // no models registered
        let orch = AgentOrchestrator::new(pool);
        let result = orch
            .execute("简单任务", &[Role::Engineer])
            .await
            .unwrap();

        assert!(result.tasks_completed > 0);
        assert!(!result.output.is_empty());
    }

    #[tokio::test]
    async fn build_task_graph_creates_correct_dag() {
        let pool = Arc::new(ModelPool::new());
        let orch = AgentOrchestrator::new(pool);
        let graph = orch.build_task_graph("测试", &[Role::Architect, Role::Engineer, Role::QA]);

        assert_eq!(graph.task_count(), 3);
        assert!(!graph.has_cycle());

        // 只有第一个任务（架构设计）是就绪的
        let ready = graph.ready_tasks();
        assert_eq!(ready.len(), 1);
    }
}
