//! 工作流引擎
//!
//! 执行用户定义的自动化工作流。

use crate::iot::GenericController;
use crate::safety::permissions::PermissionManager;
use crate::workflow::actions::ActionExecutionContext;
use crate::workflow::context::WorkflowContext;
use crate::workflow::step::{
    execute_step_with_retry, topological_order, StepExecutionRecord, StepStatus, WorkflowStep,
};
use crate::workflow::triggers::TriggerContext;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

/// 工作流定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// 工作流 ID
    pub id: Uuid,
    /// 工作流名称
    pub name: String,
    /// 触发器
    pub trigger: super::triggers::Trigger,
    /// 兼容主计划的多触发器定义
    #[serde(default)]
    pub triggers: Vec<super::triggers::Trigger>,
    /// 动作列表
    #[serde(default)]
    pub actions: Vec<super::actions::Action>,
    /// DAG 步骤列表
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
    /// 是否启用
    pub enabled: bool,
}

impl Workflow {
    fn triggers(&self) -> Vec<&super::triggers::Trigger> {
        if self.triggers.is_empty() {
            vec![&self.trigger]
        } else {
            self.triggers.iter().collect()
        }
    }

    fn matches_trigger(&self, trigger_context: &TriggerContext) -> bool {
        self.triggers()
            .into_iter()
            .any(|trigger| trigger.check(trigger_context))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowExecutionStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecutionReport {
    pub execution_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub status: WorkflowExecutionStatus,
    pub started_at: chrono::DateTime<Utc>,
    pub finished_at: chrono::DateTime<Utc>,
    pub step_records: Vec<StepExecutionRecord>,
    pub action_results: Vec<Value>,
    pub context: Value,
    pub error: Option<String>,
}

/// 工作流引擎
pub struct WorkflowEngine {
    /// 已注册工作流
    workflows: Vec<Workflow>,
    /// IoT 控制桥接
    iot_controller: Option<GenericController>,
    /// 执行权限
    permission_manager: PermissionManager,
}

impl WorkflowEngine {
    /// 创建工作流引擎
    pub fn new() -> Self {
        Self {
            workflows: Vec::new(),
            iot_controller: None,
            permission_manager: PermissionManager::new(),
        }
    }

    pub fn with_iot_controller(mut self, controller: GenericController) -> Self {
        self.iot_controller = Some(controller);
        self
    }

    pub fn with_permission_manager(mut self, permission_manager: PermissionManager) -> Self {
        self.permission_manager = permission_manager;
        self
    }

    pub fn iot_controller(&self) -> Option<&GenericController> {
        self.iot_controller.as_ref()
    }

    /// 注册工作流
    pub fn register(&mut self, workflow: Workflow) -> anyhow::Result<Uuid> {
        if workflow.name.trim().is_empty() {
            anyhow::bail!("workflow name cannot be empty");
        }
        if workflow.actions.is_empty() && workflow.steps.is_empty() {
            anyhow::bail!("workflow must define actions or steps");
        }
        if self
            .workflows
            .iter()
            .any(|current| current.id == workflow.id)
        {
            anyhow::bail!("workflow already registered: {}", workflow.id);
        }
        for trigger in workflow.triggers() {
            trigger.validate()?;
        }
        for action in &workflow.actions {
            action.validate()?;
        }
        if !workflow.steps.is_empty() {
            let mut step_keys = HashSet::new();
            for step in &workflow.steps {
                step.validate()?;
                if !step_keys.insert(step.key.as_str()) {
                    anyhow::bail!("duplicate step key: {}", step.key);
                }
            }
            let _ = topological_order(&workflow.steps)?;
        }

        let workflow_id = workflow.id;
        self.workflows.push(workflow);
        Ok(workflow_id)
    }

    /// 执行工作流
    pub async fn execute(&self, workflow_id: Uuid) -> anyhow::Result<()> {
        let manual_context = TriggerContext {
            current_time: Utc::now(),
            user_input: None,
            event_type: Some("manual".to_string()),
            event_data: None,
            webhook_path: None,
            device_state: None,
            geofence_transition: None,
        };
        let report = self
            .execute_with_trigger_context(workflow_id, &manual_context)
            .await?;
        Self::ensure_success(&report)
    }

    pub async fn execute_with_trigger_context(
        &self,
        workflow_id: Uuid,
        trigger_context: &TriggerContext,
    ) -> anyhow::Result<WorkflowExecutionReport> {
        let workflow = self
            .workflows
            .iter()
            .find(|workflow| workflow.id == workflow_id)
            .ok_or_else(|| anyhow::anyhow!("workflow not found: {workflow_id}"))?;
        if !workflow.enabled {
            anyhow::bail!("workflow is disabled: {workflow_id}");
        }

        Ok(self.run_workflow(workflow, trigger_context).await)
    }

    /// 执行所有被当前触发上下文命中的工作流
    pub async fn execute_triggered(
        &self,
        trigger_context: &TriggerContext,
    ) -> anyhow::Result<Vec<Uuid>> {
        let mut executed = Vec::new();

        for workflow in self
            .workflows
            .iter()
            .filter(|workflow| workflow.enabled && workflow.matches_trigger(trigger_context))
        {
            let report = self.run_workflow(workflow, trigger_context).await;
            Self::ensure_success(&report)?;
            executed.push(workflow.id);
        }

        Ok(executed)
    }

    /// 删除工作流
    pub fn remove(&mut self, workflow_id: Uuid) -> anyhow::Result<()> {
        let index = self
            .workflows
            .iter()
            .position(|workflow| workflow.id == workflow_id)
            .ok_or_else(|| anyhow::anyhow!("workflow not found: {workflow_id}"))?;
        self.workflows.remove(index);
        Ok(())
    }

    /// 列出所有工作流
    pub fn list(&self) -> &[Workflow] {
        &self.workflows
    }

    fn action_context(&self) -> ActionExecutionContext<'_> {
        let mut context =
            ActionExecutionContext::new().with_permission_manager(&self.permission_manager);
        if let Some(controller) = self.iot_controller.as_ref() {
            context = context.with_iot_controller(controller);
        }
        context
    }

    async fn run_workflow(
        &self,
        workflow: &Workflow,
        trigger_context: &TriggerContext,
    ) -> WorkflowExecutionReport {
        let started_at = Utc::now();
        let mut runtime_context = WorkflowContext::from_trigger_context(trigger_context);
        let mut step_records = Vec::new();
        let mut action_results = Vec::new();
        let mut error = None;

        let _ = runtime_context.set_path("workflow.id", Value::String(workflow.id.to_string()));
        let _ = runtime_context.set_path("workflow.name", Value::String(workflow.name.clone()));

        if !workflow.steps.is_empty() {
            match topological_order(&workflow.steps) {
                Ok(ordered_steps) => {
                    for step in ordered_steps {
                        let step_record = execute_step_with_retry(&step, &runtime_context).await;
                        let last_attempt = step_record.attempts.last().cloned();
                        let step_status = step_record.status.clone();
                        step_records.push(step_record);

                        match step_status {
                            StepStatus::Succeeded => {
                                if let Some(output) =
                                    last_attempt.and_then(|attempt| attempt.output)
                                {
                                    if let Err(step_error) =
                                        runtime_context.record_step_output(&step.key, output)
                                    {
                                        error = Some(step_error.to_string());
                                        break;
                                    }
                                }
                            }
                            StepStatus::Failed => {
                                error = last_attempt.and_then(|attempt| attempt.error);
                                break;
                            }
                        }
                    }
                }
                Err(graph_error) => error = Some(graph_error.to_string()),
            }
        }

        if error.is_none() {
            let action_context = self.action_context();
            for action in &workflow.actions {
                match action.execute_with_context(&action_context).await {
                    Ok(output) => action_results.push(output),
                    Err(action_error) => {
                        error = Some(action_error.to_string());
                        break;
                    }
                }
            }
        }

        let finished_at = Utc::now();
        WorkflowExecutionReport {
            execution_id: Uuid::new_v4(),
            workflow_id: workflow.id,
            workflow_name: workflow.name.clone(),
            status: if error.is_some() {
                WorkflowExecutionStatus::Failed
            } else {
                WorkflowExecutionStatus::Succeeded
            },
            started_at,
            finished_at,
            step_records,
            action_results,
            context: runtime_context.snapshot(),
            error,
        }
    }

    fn ensure_success(report: &WorkflowExecutionReport) -> anyhow::Result<()> {
        if report.status == WorkflowExecutionStatus::Failed {
            anyhow::bail!(
                "{}",
                report
                    .error
                    .clone()
                    .unwrap_or_else(|| "workflow execution failed".to_string())
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::DeviceController;
    use crate::safety::permissions::Permission;
    use crate::screen::{
        ScreenAction, ScreenActionEdge, ScreenActionGraph, ScreenActionNode, ScreenTarget,
    };
    use crate::workflow::step::{RetryPolicy, Step, WorkflowStep};
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct MockController {
        states: Arc<Mutex<HashMap<String, Value>>>,
    }

    impl MockController {
        fn new() -> Self {
            Self {
                states: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl DeviceController for MockController {
        fn supported_type(&self) -> &str {
            "mock"
        }

        async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
            self.states
                .lock()
                .unwrap()
                .insert(device_id.to_string(), json!({ "power": "on" }));
            Ok(())
        }

        async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
            self.states
                .lock()
                .unwrap()
                .insert(device_id.to_string(), json!({ "power": "off" }));
            Ok(())
        }

        async fn set_property(
            &self,
            device_id: &str,
            property: &str,
            value: Value,
        ) -> anyhow::Result<()> {
            let mut states = self.states.lock().unwrap();
            let mut current = states.get(device_id).cloned().unwrap_or_else(|| json!({}));
            current[property] = value;
            states.insert(device_id.to_string(), current);
            Ok(())
        }

        async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
            Ok(self
                .states
                .lock()
                .unwrap()
                .get(device_id)
                .cloned()
                .unwrap_or_else(|| json!({})))
        }
    }

    fn workflow(action: super::super::actions::Action) -> Workflow {
        Workflow {
            id: Uuid::new_v4(),
            name: "灯光自动化".to_string(),
            trigger: super::super::triggers::Trigger::Event {
                event_type: "manual".to_string(),
            },
            triggers: vec![],
            actions: vec![action],
            steps: vec![],
            enabled: true,
        }
    }

    #[tokio::test]
    async fn executes_control_device_action_when_permission_is_granted() {
        let mut controller = GenericController::new();
        controller.register_adapter(Box::new(MockController::new()));
        // MockController stores state in-memory, no explicit device registration needed

        let mut permissions = PermissionManager::new();
        permissions
            .request(Permission::ControlDevice("lamp-1".to_string()))
            .await
            .unwrap();

        let mut engine = WorkflowEngine::new()
            .with_iot_controller(controller)
            .with_permission_manager(permissions);
        let workflow = workflow(super::super::actions::Action::ControlDevice {
            device_id: "lamp-1".to_string(),
            command: "set_property".to_string(),
            params: json!({ "property": "brightness", "value": 64 }),
        });
        let workflow_id = workflow.id;

        engine.register(workflow).unwrap();
        engine.execute(workflow_id).await.unwrap();

        let state = engine
            .iot_controller()
            .unwrap()
            .execute("lamp-1", "get_state", json!({}))
            .await
            .unwrap();
        assert_eq!(state["brightness"], 64);
    }

    #[tokio::test]
    async fn rejects_control_device_action_without_matching_permission() {
        let mut controller = GenericController::new();
        controller.register_adapter(Box::new(MockController::new()));

        let mut engine = WorkflowEngine::new().with_iot_controller(controller);
        let workflow = workflow(super::super::actions::Action::ControlDevice {
            device_id: "lamp-2".to_string(),
            command: "turn_on".to_string(),
            params: json!({}),
        });
        let workflow_id = workflow.id;

        engine.register(workflow).unwrap();
        let error = engine.execute(workflow_id).await.unwrap_err();

        assert!(error.to_string().contains("permission"));
    }

    #[test]
    fn register_rejects_invalid_control_device_action() {
        let mut engine = WorkflowEngine::new();
        let workflow = workflow(super::super::actions::Action::ControlDevice {
            device_id: "".to_string(),
            command: "turn_on".to_string(),
            params: json!({}),
        });

        let error = engine.register(workflow).unwrap_err();
        assert!(error.to_string().contains("device_id"));
    }

    #[tokio::test]
    async fn executes_screen_plan_action_when_screen_permissions_are_granted() {
        let observe_id = Uuid::new_v4();
        let click_id = Uuid::new_v4();
        let mut graph = ScreenActionGraph::new();
        graph
            .add_node(ScreenActionNode {
                id: observe_id,
                action: ScreenAction::Observe {
                    target: ScreenTarget::Window {
                        app: "Browser".to_string(),
                        title: Some("Checkout".to_string()),
                    },
                },
            })
            .unwrap();
        graph
            .add_node(ScreenActionNode {
                id: click_id,
                action: ScreenAction::Click {
                    target: ScreenTarget::Element {
                        selector: "#submit".to_string(),
                    },
                },
            })
            .unwrap();
        graph
            .add_edge(ScreenActionEdge {
                from: observe_id,
                to: click_id,
            })
            .unwrap();

        let mut permissions = PermissionManager::new();
        permissions
            .request(Permission::ObserveScreen)
            .await
            .unwrap();
        permissions
            .request(Permission::ControlScreen)
            .await
            .unwrap();

        let mut engine = WorkflowEngine::new().with_permission_manager(permissions);
        let workflow = workflow(super::super::actions::Action::ValidateScreenPlan { graph });
        let workflow_id = workflow.id;

        engine.register(workflow).unwrap();
        engine.execute(workflow_id).await.unwrap();
    }

    #[tokio::test]
    async fn rejects_screen_plan_action_without_required_screen_permissions() {
        let mut graph = ScreenActionGraph::new();
        graph
            .add_node(ScreenActionNode {
                id: Uuid::new_v4(),
                action: ScreenAction::Extract {
                    target: ScreenTarget::Element {
                        selector: ".summary".to_string(),
                    },
                    schema: "text".to_string(),
                },
            })
            .unwrap();

        let mut engine = WorkflowEngine::new();
        let workflow = workflow(super::super::actions::Action::ValidateScreenPlan { graph });
        let workflow_id = workflow.id;

        engine.register(workflow).unwrap();
        let error = engine.execute(workflow_id).await.unwrap_err();
        assert!(error.to_string().contains("missing permission"));
    }

    #[test]
    fn register_rejects_cyclic_screen_plan_action() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut graph = ScreenActionGraph::new();
        graph
            .add_node(ScreenActionNode {
                id: first,
                action: ScreenAction::Observe {
                    target: ScreenTarget::Element {
                        selector: "#search".to_string(),
                    },
                },
            })
            .unwrap();
        graph
            .add_node(ScreenActionNode {
                id: second,
                action: ScreenAction::Extract {
                    target: ScreenTarget::Element {
                        selector: "#result".to_string(),
                    },
                    schema: "json".to_string(),
                },
            })
            .unwrap();
        graph
            .add_edge(ScreenActionEdge {
                from: first,
                to: second,
            })
            .unwrap();
        graph
            .add_edge(ScreenActionEdge {
                from: second,
                to: first,
            })
            .unwrap();

        let mut engine = WorkflowEngine::new();
        let workflow = workflow(super::super::actions::Action::ValidateScreenPlan { graph });

        let error = engine.register(workflow).unwrap_err();
        assert!(error.to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn execute_triggered_runs_only_matching_enabled_workflows() {
        let mut controller = GenericController::new();
        controller.register_adapter(Box::new(MockController::new()));

        let mut permissions = PermissionManager::new();
        permissions
            .request(Permission::ControlDevice("lamp-triggered".to_string()))
            .await
            .unwrap();

        let mut engine = WorkflowEngine::new()
            .with_iot_controller(controller)
            .with_permission_manager(permissions);

        let matching = Workflow {
            id: Uuid::new_v4(),
            name: "presence automation".to_string(),
            trigger: super::super::triggers::Trigger::Event {
                event_type: "presence.detected".to_string(),
            },
            triggers: vec![],
            actions: vec![super::super::actions::Action::ControlDevice {
                device_id: "lamp-triggered".to_string(),
                command: "set_property".to_string(),
                params: json!({ "property": "brightness", "value": 30 }),
            }],
            steps: vec![],
            enabled: true,
        };

        let skipped = Workflow {
            id: Uuid::new_v4(),
            name: "disabled automation".to_string(),
            trigger: super::super::triggers::Trigger::Event {
                event_type: "presence.detected".to_string(),
            },
            triggers: vec![],
            actions: vec![super::super::actions::Action::ControlDevice {
                device_id: "lamp-triggered".to_string(),
                command: "set_property".to_string(),
                params: json!({ "property": "brightness", "value": 90 }),
            }],
            steps: vec![],
            enabled: false,
        };

        let non_matching = Workflow {
            id: Uuid::new_v4(),
            name: "door automation".to_string(),
            trigger: super::super::triggers::Trigger::Event {
                event_type: "door.opened".to_string(),
            },
            triggers: vec![],
            actions: vec![super::super::actions::Action::ControlDevice {
                device_id: "lamp-triggered".to_string(),
                command: "set_property".to_string(),
                params: json!({ "property": "brightness", "value": 80 }),
            }],
            steps: vec![],
            enabled: true,
        };

        let matching_id = matching.id;
        engine.register(matching).unwrap();
        engine.register(skipped).unwrap();
        engine.register(non_matching).unwrap();

        let executed = engine
            .execute_triggered(&super::super::triggers::TriggerContext {
                current_time: chrono::Utc::now(),
                user_input: None,
                event_type: Some("presence.detected".to_string()),
                event_data: None,
                webhook_path: None,
                device_state: None,
                geofence_transition: None,
            })
            .await
            .unwrap();

        assert_eq!(executed, vec![matching_id]);

        let state = engine
            .iot_controller()
            .unwrap()
            .execute("lamp-triggered", "get_state", json!({}))
            .await
            .unwrap();
        assert_eq!(state["brightness"], 30);
    }

    #[tokio::test]
    async fn executes_dag_steps_with_data_passing_and_condition_branching() {
        let extract_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let draft_id = Uuid::new_v4();

        let workflow = Workflow {
            id: Uuid::new_v4(),
            name: "每日摘要".to_string(),
            trigger: super::super::triggers::Trigger::Manual,
            triggers: vec![],
            actions: vec![],
            steps: vec![
                WorkflowStep {
                    id: extract_id,
                    name: "Extract summary".to_string(),
                    key: "summary".to_string(),
                    step: Step::Transform {
                        jq_expression: ".trigger.event_data.summary".to_string(),
                    },
                    depends_on: vec![],
                    timeout_secs: 30,
                    retry_policy: RetryPolicy::default(),
                },
                WorkflowStep {
                    id: branch_id,
                    name: "Branch".to_string(),
                    key: "branch".to_string(),
                    step: Step::Condition {
                        expression: "eq(trigger.event_type, \"daily.summary\")".to_string(),
                        if_true: json!("Daily digest"),
                        if_false: json!("Update"),
                    },
                    depends_on: vec![],
                    timeout_secs: 30,
                    retry_policy: RetryPolicy::default(),
                },
                WorkflowStep {
                    id: draft_id,
                    name: "Draft".to_string(),
                    key: "draft".to_string(),
                    step: Step::LlmCall {
                        prompt_template: "{{steps.branch}}: {{steps.summary}}".to_string(),
                        model_preference: Some("local".to_string()),
                    },
                    depends_on: vec![extract_id, branch_id],
                    timeout_secs: 30,
                    retry_policy: RetryPolicy::default(),
                },
                WorkflowStep {
                    id: Uuid::new_v4(),
                    name: "Send".to_string(),
                    key: "send".to_string(),
                    step: Step::SendMessage {
                        channel: "telegram".to_string(),
                        template: "{{steps.draft.text}}".to_string(),
                    },
                    depends_on: vec![draft_id],
                    timeout_secs: 30,
                    retry_policy: RetryPolicy::default(),
                },
            ],
            enabled: true,
        };

        let workflow_id = workflow.id;
        let mut engine = WorkflowEngine::new();
        engine.register(workflow).unwrap();

        let report = engine
            .execute_with_trigger_context(
                workflow_id,
                &super::super::triggers::TriggerContext {
                    current_time: chrono::Utc::now(),
                    user_input: None,
                    event_type: Some("daily.summary".to_string()),
                    event_data: Some("{\"summary\":\"Revenue up 12%\"}".to_string()),
                    webhook_path: None,
                    device_state: None,
                    geofence_transition: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(report.status, WorkflowExecutionStatus::Succeeded);
        let step_order = report
            .step_records
            .iter()
            .map(|record| record.step_key.as_str())
            .collect::<Vec<_>>();
        let summary_index = step_order.iter().position(|key| *key == "summary").unwrap();
        let branch_index = step_order.iter().position(|key| *key == "branch").unwrap();
        let draft_index = step_order.iter().position(|key| *key == "draft").unwrap();
        let send_index = step_order.iter().position(|key| *key == "send").unwrap();
        assert!(summary_index < draft_index);
        assert!(branch_index < draft_index);
        assert!(draft_index < send_index);
        assert_eq!(report.context["steps"]["branch"], "Daily digest");
        assert_eq!(
            report.context["steps"]["send"]["channel"],
            Value::String("telegram".to_string())
        );
        assert!(report.context["steps"]["send"]["message"]
            .as_str()
            .unwrap()
            .contains("Daily digest: Revenue up 12%"));
    }
}
