//! 任务调度器
//!
//! 定时任务调度，支持 cron 表达式和一次性任务。

use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

/// 调度任务
#[derive(Debug, Clone)]
pub struct ScheduledTask {
    /// 任务 ID
    pub id: Uuid,
    /// 任务名称
    pub name: String,
    /// Cron 表达式
    pub cron: Option<String>,
    /// 一次性执行时间
    pub run_at: Option<DateTime<Utc>>,
    /// 是否启用
    pub enabled: bool,
}

/// 任务调度器
pub struct TaskScheduler {
    /// 已注册任务
    tasks: Vec<ScheduledTask>,
    /// 运行状态
    running: AtomicBool,
}

impl TaskScheduler {
    /// 创建任务调度器
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            running: AtomicBool::new(false),
        }
    }

    /// 添加定时任务
    pub fn schedule(&mut self, task: ScheduledTask) -> anyhow::Result<Uuid> {
        if task.name.trim().is_empty() {
            anyhow::bail!("task name cannot be empty");
        }
        match (task.cron.as_ref(), task.run_at.as_ref()) {
            (Some(cron), None) => {
                if !crate::workflow::triggers::is_valid_schedule_expression(cron) {
                    anyhow::bail!("invalid cron expression");
                }
            }
            (None, Some(_)) => {}
            (None, None) => anyhow::bail!("task must define cron or run_at"),
            (Some(_), Some(_)) => anyhow::bail!("task must define exactly one schedule mode"),
        }
        if self.tasks.iter().any(|current| current.id == task.id) {
            anyhow::bail!("task already scheduled: {}", task.id);
        }

        let task_id = task.id;
        self.tasks.push(task);
        Ok(task_id)
    }

    /// 取消任务
    pub fn cancel(&mut self, task_id: Uuid) -> anyhow::Result<()> {
        let index = self
            .tasks
            .iter()
            .position(|task| task.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {task_id}"))?;
        self.tasks.remove(index);
        Ok(())
    }

    /// 启动调度器
    pub async fn start(&self) -> anyhow::Result<()> {
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// 停止调度器
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// 列出所有任务
    pub fn list_tasks(&self) -> &[ScheduledTask] {
        &self.tasks
    }

    /// 返回当前是否处于运行态
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 返回当前时刻到期的任务快照
    pub fn due_tasks(&self, now: DateTime<Utc>) -> Vec<ScheduledTask> {
        self.tasks
            .iter()
            .filter(|task| task.enabled)
            .filter(|task| {
                task.run_at.is_some_and(|run_at| run_at <= now)
                    || task.cron.as_ref().is_some_and(|cron| {
                        crate::workflow::Trigger::Schedule { cron: cron.clone() }.check(
                            &crate::workflow::triggers::TriggerContext {
                                current_time: now,
                                user_input: None,
                                event_type: None,
                                event_data: None,
                                webhook_path: None,
                                device_state: None,
                                geofence_transition: None,
                            },
                        )
                    })
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn task(
        name: &str,
        cron: Option<&str>,
        run_at: Option<DateTime<Utc>>,
        enabled: bool,
    ) -> ScheduledTask {
        ScheduledTask {
            id: Uuid::new_v4(),
            name: name.to_string(),
            cron: cron.map(str::to_string),
            run_at,
            enabled,
        }
    }

    #[test]
    fn schedule_registers_valid_tasks_and_rejects_invalid_definitions() {
        let mut scheduler = TaskScheduler::new();
        let cron_task = task("weekly sync", Some("30 9 * * 1"), None, true);
        let run_once = task(
            "one shot",
            None,
            Some(Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap()),
            true,
        );

        assert_eq!(scheduler.schedule(cron_task.clone()).unwrap(), cron_task.id);
        assert_eq!(scheduler.schedule(run_once.clone()).unwrap(), run_once.id);

        let missing_schedule = task("invalid", None, None, true);
        let error = scheduler.schedule(missing_schedule).unwrap_err();
        assert!(error.to_string().contains("cron or run_at"));

        let both_modes = task(
            "ambiguous",
            Some("* * * * *"),
            Some(Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap()),
            true,
        );
        let error = scheduler.schedule(both_modes).unwrap_err();
        assert!(error.to_string().contains("exactly one"));

        let duplicate = cron_task.clone();
        let error = scheduler.schedule(duplicate).unwrap_err();
        assert!(error.to_string().contains("already scheduled"));
    }

    #[test]
    fn cancel_removes_task_and_due_tasks_filter_by_time_and_enabled_state() {
        let mut scheduler = TaskScheduler::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 45).unwrap();
        let cron_task = task("weekly sync", Some("30 9 * * 1"), None, true);
        let one_shot = task(
            "one shot",
            None,
            Some(Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap()),
            true,
        );
        let disabled = task(
            "disabled",
            None,
            Some(Utc.with_ymd_and_hms(2026, 3, 9, 9, 29, 0).unwrap()),
            false,
        );

        let cron_id = cron_task.id;
        let one_shot_id = one_shot.id;
        scheduler.schedule(cron_task).unwrap();
        scheduler.schedule(one_shot).unwrap();
        scheduler.schedule(disabled).unwrap();

        let due = scheduler.due_tasks(now);
        let due_ids = due.iter().map(|task| task.id).collect::<Vec<_>>();
        assert!(due_ids.contains(&cron_id));
        assert!(due_ids.contains(&one_shot_id));
        assert_eq!(due_ids.len(), 2);

        scheduler.cancel(one_shot_id).unwrap();
        assert!(!scheduler
            .list_tasks()
            .iter()
            .any(|task| task.id == one_shot_id));
    }

    #[tokio::test]
    async fn start_and_stop_flip_running_state() {
        let scheduler = TaskScheduler::new();
        assert!(!scheduler.is_running());

        scheduler.start().await.unwrap();
        assert!(scheduler.is_running());

        scheduler.stop();
        assert!(!scheduler.is_running());
    }

    #[test]
    fn scheduler_add_remove_task() {
        let mut scheduler = TaskScheduler::new();
        let t = task(
            "removable",
            None,
            Some(Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap()),
            true,
        );
        let id = scheduler.schedule(t).unwrap();
        assert_eq!(scheduler.list_tasks().len(), 1);

        scheduler.cancel(id).unwrap();
        assert!(scheduler.list_tasks().is_empty());
    }

    #[test]
    fn scheduler_due_tasks_respects_enabled_flag() {
        let mut scheduler = TaskScheduler::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();
        let enabled = task(
            "enabled",
            None,
            Some(Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap()),
            true,
        );
        let disabled = task(
            "disabled",
            None,
            Some(Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap()),
            false,
        );
        scheduler.schedule(enabled).unwrap();
        scheduler.schedule(disabled).unwrap();

        let due = scheduler.due_tasks(now);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].name, "enabled");
    }

    #[test]
    fn scheduler_invalid_cron_rejected() {
        let mut scheduler = TaskScheduler::new();
        let bad_cron = task("bad", Some("not a cron"), None, true);
        let err = scheduler.schedule(bad_cron).unwrap_err();
        assert!(err.to_string().contains("invalid cron"));
    }
}
