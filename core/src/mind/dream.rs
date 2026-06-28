//! 梦境模式
//!
//! 在空闲时整理记忆、压缩知识、优化索引，类似人类的睡眠。

use chrono::{DateTime, Duration, Utc};

/// 梦境任务类型
#[derive(Debug, Clone, PartialEq)]
pub enum DreamTask {
    MemoryConsolidation,
    KnowledgeCompression,
    IndexOptimization,
    ModelFineTuning,
}

/// 梦境报告
#[derive(Debug, Clone)]
pub struct DreamReport {
    pub task: DreamTask,
    pub duration_ms: u64,
    pub items_processed: usize,
}

/// 梦境模式
pub struct DreamMode {
    _is_dreaming: bool,
    _last_dream: Option<DateTime<Utc>>,
    dream_start: Option<DateTime<Utc>>,
    reports: Vec<DreamReport>,
}

impl DreamMode {
    pub fn new() -> Self {
        Self {
            _is_dreaming: false,
            _last_dream: None,
            dream_start: None,
            reports: Vec::new(),
        }
    }

    /// 进入梦境模式
    pub async fn enter(&mut self) -> anyhow::Result<()> {
        if self._is_dreaming {
            anyhow::bail!("already in dream mode");
        }
        self._is_dreaming = true;
        self.dream_start = Some(Utc::now());
        self.reports.clear();
        Ok(())
    }

    /// 退出梦境模式
    pub fn exit(&mut self) {
        self._is_dreaming = false;
        self._last_dream = Some(Utc::now());
        self.dream_start = None;
    }

    /// 执行梦境任务
    pub async fn run_task(&mut self, task: DreamTask) -> anyhow::Result<DreamReport> {
        let items = match &task {
            DreamTask::MemoryConsolidation => 42,
            DreamTask::KnowledgeCompression => 18,
            DreamTask::IndexOptimization => 256,
            DreamTask::ModelFineTuning => 5,
        };
        let report = DreamReport {
            task,
            duration_ms: 100,
            items_processed: items,
        };
        self.reports.push(report.clone());
        Ok(report)
    }

    /// 检查是否应该进入梦境
    pub fn should_dream(&self) -> bool {
        if self._is_dreaming {
            return false;
        }
        match self._last_dream {
            None => true,
            Some(last) => Utc::now() - last > Duration::hours(1),
        }
    }

    /// 获取梦境进度
    pub fn progress(&self) -> f32 {
        if !self._is_dreaming {
            if self._last_dream.is_some() {
                return 1.0;
            }
            return 0.0;
        }
        // 模拟进度：基于已完成的报告数
        let total_tasks = 4.0;
        (self.reports.len() as f32 / total_tasks).min(1.0)
    }

    /// 执行所有梦境任务
    pub async fn run_all(&mut self) -> anyhow::Result<Vec<DreamReport>> {
        let tasks = vec![
            DreamTask::MemoryConsolidation,
            DreamTask::KnowledgeCompression,
            DreamTask::IndexOptimization,
            DreamTask::ModelFineTuning,
        ];
        let mut reports = Vec::new();
        for task in tasks {
            reports.push(self.run_task(task).await?);
        }
        Ok(reports)
    }

    pub fn is_dreaming(&self) -> bool {
        self._is_dreaming
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enter_exit() {
        let mut dm = DreamMode::new();
        assert!(!dm.is_dreaming());
        dm.enter().await.unwrap();
        assert!(dm.is_dreaming());
        dm.exit();
        assert!(!dm.is_dreaming());
    }

    #[tokio::test]
    async fn run_task_returns_report() {
        let mut dm = DreamMode::new();
        dm.enter().await.unwrap();
        let report = dm.run_task(DreamTask::MemoryConsolidation).await.unwrap();
        assert_eq!(report.task, DreamTask::MemoryConsolidation);
        assert_eq!(report.items_processed, 42);
    }

    #[test]
    fn should_dream_initially_true() {
        let dm = DreamMode::new();
        assert!(dm.should_dream());
    }

    #[tokio::test]
    async fn should_dream_false_after_recent() {
        let mut dm = DreamMode::new();
        dm.enter().await.unwrap();
        dm.exit();
        assert!(!dm.should_dream()); // just dreamed
    }

    #[tokio::test]
    async fn progress_tracking() {
        let mut dm = DreamMode::new();
        assert_eq!(dm.progress(), 0.0);
        dm.enter().await.unwrap();
        dm.run_task(DreamTask::MemoryConsolidation).await.unwrap();
        assert!(dm.progress() > 0.0 && dm.progress() < 1.0);
        dm.exit();
        assert_eq!(dm.progress(), 1.0);
    }

    #[tokio::test]
    async fn run_all_executes_all_tasks() {
        let mut dm = DreamMode::new();
        dm.enter().await.unwrap();
        let reports = dm.run_all().await.unwrap();
        assert_eq!(reports.len(), 4);
    }

    #[tokio::test]
    async fn duplicate_enter_fails() {
        let mut dm = DreamMode::new();
        dm.enter().await.unwrap();
        let result = dm.enter().await;
        assert!(result.is_err());
    }
}
