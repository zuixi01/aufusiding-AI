//! 工作流调度器。

use crate::workflow::{WorkflowEngine, WorkflowExecutionReport};
use chrono::{DateTime, Duration, Utc};
use cron::Schedule;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct WorkflowScheduler {
    history: Vec<WorkflowExecutionReport>,
}

impl WorkflowScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn due_workflow_ids(
        &self,
        engine: &WorkflowEngine,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Vec<Uuid>> {
        let mut due = Vec::new();

        for workflow in engine.list().iter().filter(|workflow| workflow.enabled) {
            let triggers = if workflow.triggers.is_empty() {
                vec![&workflow.trigger]
            } else {
                workflow.triggers.iter().collect::<Vec<_>>()
            };

            let mut is_due = false;
            for trigger in triggers {
                if let crate::workflow::Trigger::Schedule { cron } = trigger {
                    if cron_matches_now(cron, now)? {
                        is_due = true;
                        break;
                    }
                }
            }

            if is_due {
                due.push(workflow.id);
            }
        }

        Ok(due)
    }

    pub async fn run_due(
        &mut self,
        engine: &WorkflowEngine,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Vec<Uuid>> {
        let due = self.due_workflow_ids(engine, now)?;

        for workflow_id in &due {
            let report = engine
                .execute_with_trigger_context(
                    *workflow_id,
                    &crate::workflow::triggers::TriggerContext {
                        current_time: now,
                        user_input: None,
                        event_type: Some("schedule".to_string()),
                        event_data: None,
                        webhook_path: None,
                        device_state: None,
                        geofence_transition: None,
                    },
                )
                .await?;
            self.history.push(report);
        }

        Ok(due)
    }

    pub fn history(&self) -> &[WorkflowExecutionReport] {
        &self.history
    }

    pub fn history_for(&self, workflow_id: Uuid) -> Vec<&WorkflowExecutionReport> {
        self.history
            .iter()
            .filter(|record| record.workflow_id == workflow_id)
            .collect()
    }
}

fn cron_matches_now(cron: &str, now: DateTime<Utc>) -> anyhow::Result<bool> {
    let normalized = normalize_cron_expression(cron)?;
    let schedule = Schedule::from_str(&normalized)?;
    let window_start = now - Duration::minutes(1);
    Ok(schedule
        .after(&window_start)
        .next()
        .is_some_and(|next| next <= now))
}

fn normalize_cron_expression(cron: &str) -> anyhow::Result<String> {
    let fields = cron.split_whitespace().collect::<Vec<_>>();
    match fields.len() {
        5 => Ok(format!("0 {}", cron.trim())),
        6 => Ok(cron.trim().to_string()),
        7 => Ok(cron.trim().to_string()),
        _ => anyhow::bail!("unsupported cron expression: {cron}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{
        RetryPolicy, Step, Trigger, Workflow, WorkflowExecutionStatus, WorkflowStep,
    };
    use chrono::TimeZone;

    fn step(
        name: &str,
        key: &str,
        step: Step,
        depends_on: Vec<Uuid>,
        retry_policy: RetryPolicy,
    ) -> WorkflowStep {
        WorkflowStep {
            id: Uuid::new_v4(),
            name: name.to_string(),
            key: key.to_string(),
            step,
            depends_on,
            timeout_secs: 30,
            retry_policy,
        }
    }

    #[tokio::test]
    async fn runs_daily_summary_workflow_on_due_cron_and_records_history() {
        let summary_step = WorkflowStep {
            id: Uuid::new_v4(),
            name: "Generate summary".to_string(),
            key: "summary".to_string(),
            step: Step::LlmCall {
                prompt_template: "Generate daily summary for {{trigger.current_time}}".to_string(),
                model_preference: Some("local".to_string()),
            },
            depends_on: vec![],
            timeout_secs: 30,
            retry_policy: RetryPolicy::default(),
        };
        let workflow = Workflow {
            id: Uuid::new_v4(),
            name: "每日摘要".to_string(),
            trigger: Trigger::Schedule {
                cron: "0 9 * * *".to_string(),
            },
            triggers: vec![],
            actions: vec![],
            steps: vec![
                summary_step.clone(),
                step(
                    "Send summary",
                    "send",
                    Step::SendMessage {
                        channel: "telegram".to_string(),
                        template: "{{steps.summary.text}}".to_string(),
                    },
                    vec![summary_step.id],
                    RetryPolicy::default(),
                ),
            ],
            enabled: true,
        };

        let workflow_id = workflow.id;
        let mut engine = WorkflowEngine::new();
        engine.register(workflow).unwrap();

        let mut scheduler = WorkflowScheduler::new();
        let executed = scheduler
            .run_due(
                &engine,
                Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 30).unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(executed, vec![workflow_id]);
        let history = scheduler.history_for(workflow_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, WorkflowExecutionStatus::Succeeded);
        assert_eq!(history[0].step_records.len(), 2);
        assert_eq!(history[0].context["steps"]["send"]["channel"], "telegram");
    }

    #[tokio::test]
    async fn retries_failed_step_and_keeps_failure_in_history() {
        let workflow = Workflow {
            id: Uuid::new_v4(),
            name: "失败重试".to_string(),
            trigger: Trigger::Schedule {
                cron: "0 9 * * *".to_string(),
            },
            triggers: vec![],
            actions: vec![],
            steps: vec![step(
                "Broken transform",
                "broken",
                Step::Transform {
                    jq_expression: ".steps.missing.payload".to_string(),
                },
                vec![],
                RetryPolicy {
                    max_attempts: 2,
                    backoff_ms: 0,
                },
            )],
            enabled: true,
        };

        let workflow_id = workflow.id;
        let mut engine = WorkflowEngine::new();
        engine.register(workflow).unwrap();

        let mut scheduler = WorkflowScheduler::new();
        let executed = scheduler
            .run_due(
                &engine,
                Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 30).unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(executed, vec![workflow_id]);
        let history = scheduler.history_for(workflow_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, WorkflowExecutionStatus::Failed);
        assert_eq!(history[0].step_records[0].attempts.len(), 2);
        assert!(history[0]
            .error
            .as_deref()
            .unwrap()
            .contains("transform path not found"));
    }

    #[test]
    fn due_workflow_ids_only_returns_enabled_schedule_workflows() {
        let scheduled = Workflow {
            id: Uuid::new_v4(),
            name: "scheduled".to_string(),
            trigger: Trigger::Schedule {
                cron: "0 9 * * *".to_string(),
            },
            triggers: vec![],
            actions: vec![],
            steps: vec![step(
                "Generate",
                "generate",
                Step::LlmCall {
                    prompt_template: "ping".to_string(),
                    model_preference: Some("local".to_string()),
                },
                vec![],
                RetryPolicy::default(),
            )],
            enabled: true,
        };
        let manual = Workflow {
            id: Uuid::new_v4(),
            name: "manual".to_string(),
            trigger: Trigger::Manual,
            triggers: vec![],
            actions: vec![],
            steps: vec![step(
                "Generate",
                "generate_manual",
                Step::LlmCall {
                    prompt_template: "ping".to_string(),
                    model_preference: Some("local".to_string()),
                },
                vec![],
                RetryPolicy::default(),
            )],
            enabled: true,
        };
        let disabled = Workflow {
            id: Uuid::new_v4(),
            name: "disabled".to_string(),
            trigger: Trigger::Schedule {
                cron: "0 9 * * *".to_string(),
            },
            triggers: vec![],
            actions: vec![],
            steps: vec![step(
                "Generate",
                "generate_disabled",
                Step::LlmCall {
                    prompt_template: "ping".to_string(),
                    model_preference: Some("local".to_string()),
                },
                vec![],
                RetryPolicy::default(),
            )],
            enabled: false,
        };

        let expected_id = scheduled.id;
        let mut engine = WorkflowEngine::new();
        engine.register(scheduled).unwrap();
        engine.register(manual).unwrap();
        engine.register(disabled).unwrap();

        let scheduler = WorkflowScheduler::new();
        let due = scheduler
            .due_workflow_ids(
                &engine,
                Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 30).unwrap(),
            )
            .unwrap();

        assert_eq!(due, vec![expected_id]);
    }
}
