//! 跨设备任务交接
//!
//! 将当前任务上下文转移到目标设备（手机→车机→家庭音箱）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 任务上下文（用于跨设备传递）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_id: Uuid,
    pub description: String,
    pub state: serde_json::Value,
    pub source_device: String,
    pub created_at: DateTime<Utc>,
}

/// 交接记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffRecord {
    pub id: Uuid,
    pub task_context: TaskContext,
    pub target_device: String,
    pub timestamp: DateTime<Utc>,
    pub status: HandoffStatus,
}

/// 交接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffStatus {
    Pending,
    Transferred,
    Accepted,
    Failed,
}

/// 设备交接服务
pub struct DeviceHandoff {
    records: std::sync::Mutex<Vec<HandoffRecord>>,
}

impl DeviceHandoff {
    pub fn new() -> Self {
        Self {
            records: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 将任务上下文转移到目标设备
    pub async fn transfer(
        &self,
        task_context: TaskContext,
        target_device: &str,
    ) -> anyhow::Result<Uuid> {
        let record = HandoffRecord {
            id: Uuid::new_v4(),
            task_context,
            target_device: target_device.to_string(),
            timestamp: Utc::now(),
            status: HandoffStatus::Transferred,
        };

        tracing::info!(
            target = %target_device,
            task = %record.task_context.description,
            "任务交接"
        );

        let id = record.id;
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(record);
        Ok(id)
    }

    /// 确认接收
    pub fn accept(&self, handoff_id: Uuid) -> anyhow::Result<()> {
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        let record = records
            .iter_mut()
            .find(|r| r.id == handoff_id)
            .ok_or_else(|| anyhow::anyhow!("交接记录不存在: {}", handoff_id))?;
        record.status = HandoffStatus::Accepted;
        Ok(())
    }

    /// 获取待接收的交接
    pub fn pending_for_device(&self, device_id: &str) -> Vec<HandoffRecord> {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|r| r.target_device == device_id && r.status == HandoffStatus::Transferred)
            .cloned()
            .collect()
    }

    /// 交接记录数
    pub fn record_count(&self) -> usize {
        self.records.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}
