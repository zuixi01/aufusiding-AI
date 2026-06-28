//! 异步审计日志写入器
//!
//! 通过 `tokio::sync::mpsc` 将审计事件异步写入，避免在 async 处理函数中阻塞 Tokio 线程。

use prometheus_core::security::{SecurityAuditLog, SecurityEvent};
use std::sync::Arc;
use tokio::sync::mpsc;

/// 异步审计日志写入器（可 Clone，线程安全）
#[derive(Clone)]
pub struct AuditWriter {
    tx: mpsc::Sender<SecurityEvent>,
}

impl AuditWriter {
    /// 非阻塞发送审计事件。通道满时丢弃事件并记录警告。
    pub fn send(&self, event: SecurityEvent) {
        if let Err(mpsc::error::TrySendError::Full(_)) = self.tx.try_send(event) {
            tracing::warn!("审计日志通道已满，事件被丢弃");
        }
    }
}

/// 启动异步审计写入后台任务，返回 AuditWriter 句柄。
pub fn spawn_audit_writer(log: Arc<SecurityAuditLog>) -> AuditWriter {
    let (tx, mut rx) = mpsc::channel::<SecurityEvent>(512);
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let log = log.clone();
            let _ = tokio::task::spawn_blocking(move || {
                if let Err(e) = log.log_event(event) {
                    tracing::error!("审计日志写入失败: {}", e);
                }
            })
            .await;
        }
    });
    AuditWriter { tx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus_core::security::{SecurityEventType, Severity};
    use std::time::Duration;

    #[tokio::test]
    async fn audit_writer_records_events_async() {
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(SecurityAuditLog::new(dir.path().join("audit.db")).unwrap());
        let writer = spawn_audit_writer(log.clone());

        writer.send(SecurityEvent {
            timestamp: chrono::Utc::now(),
            event_type: SecurityEventType::PromptInjectionAttempt,
            severity: Severity::High,
            client_id: Some("test-client".into()),
            details: serde_json::json!({"test": true}),
            action_taken: "blocked".into(),
        });

        // 等待后台写入完成
        tokio::time::sleep(Duration::from_millis(200)).await;

        let count = log.count_events().unwrap();
        assert!(count >= 1, "应至少记录 1 个事件, 实际: {}", count);
    }

    #[tokio::test]
    async fn audit_writer_handles_multiple_events() {
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(SecurityAuditLog::new(dir.path().join("audit.db")).unwrap());
        let writer = spawn_audit_writer(log.clone());

        for i in 0..5 {
            writer.send(SecurityEvent {
                timestamp: chrono::Utc::now(),
                event_type: SecurityEventType::InvalidInput,
                severity: Severity::Medium,
                client_id: Some(format!("client-{}", i)),
                details: serde_json::json!({"index": i}),
                action_taken: "rejected".into(),
            });
        }

        tokio::time::sleep(Duration::from_millis(500)).await;

        let count = log.count_events().unwrap();
        assert_eq!(count, 5, "应记录 5 个事件, 实际: {}", count);
    }
}
