//! 安全审计日志
//!
//! 记录所有安全相关事件，用于监控和事后分析。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::storage::db::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: SecurityEventType,
    pub severity: Severity,
    pub client_id: Option<String>,
    pub details: serde_json::Value,
    pub action_taken: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEventType {
    PromptInjectionAttempt,
    SqlInjectionAttempt,
    CommandInjectionAttempt,
    RateLimitExceeded,
    InvalidInput,
    UnauthorizedAccess,
    SuspiciousActivity,
    AuthenticationFailure,
    DataAccessViolation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

fn parse_event_type(s: &str) -> SecurityEventType {
    match s {
        "PromptInjectionAttempt" => SecurityEventType::PromptInjectionAttempt,
        "SqlInjectionAttempt" => SecurityEventType::SqlInjectionAttempt,
        "CommandInjectionAttempt" => SecurityEventType::CommandInjectionAttempt,
        "RateLimitExceeded" => SecurityEventType::RateLimitExceeded,
        "InvalidInput" => SecurityEventType::InvalidInput,
        "UnauthorizedAccess" => SecurityEventType::UnauthorizedAccess,
        "SuspiciousActivity" => SecurityEventType::SuspiciousActivity,
        "AuthenticationFailure" => SecurityEventType::AuthenticationFailure,
        "DataAccessViolation" => SecurityEventType::DataAccessViolation,
        _ => SecurityEventType::SuspiciousActivity,
    }
}

fn parse_severity(s: &str) -> Severity {
    match s {
        "Critical" => Severity::Critical,
        "High" => Severity::High,
        "Medium" => Severity::Medium,
        "Low" => Severity::Low,
        "Info" => Severity::Info,
        _ => Severity::Info,
    }
}

pub struct SecurityAuditLog {
    db: Database,
}

impl SecurityAuditLog {
    /// 创建或打开安全审计日志
    pub fn new(db_path: std::path::PathBuf) -> anyhow::Result<Self> {
        let db = Database::open(db_path)?;

        // 创建表
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS security_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                severity TEXT NOT NULL,
                client_id TEXT,
                details TEXT NOT NULL,
                action_taken TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_se_timestamp ON security_events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_se_severity ON security_events(severity);
            CREATE INDEX IF NOT EXISTS idx_se_client_id ON security_events(client_id);",
        )?;

        Ok(Self { db })
    }

    /// 记录安全事件
    pub fn log_event(&self, event: SecurityEvent) -> anyhow::Result<()> {
        self.db.execute(
            "INSERT INTO security_events (timestamp, event_type, severity, client_id, details, action_taken)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                event.timestamp.timestamp(),
                format!("{:?}", event.event_type),
                format!("{:?}", event.severity),
                event.client_id,
                event.details.to_string(),
                event.action_taken,
            ],
        )?;

        // 高危事件记录到日志
        if matches!(event.severity, Severity::Critical | Severity::High) {
            tracing::error!(
                event_type = ?event.event_type,
                severity = ?event.severity,
                client_id = ?event.client_id,
                "安全事件"
            );
        }

        Ok(())
    }

    /// 查询最近的安全事件
    pub fn query_recent(&self, limit: usize) -> anyhow::Result<Vec<SecurityEvent>> {
        self.db.query_map(
            "SELECT timestamp, event_type, severity, client_id, details, action_taken
             FROM security_events ORDER BY timestamp DESC LIMIT ?1",
            &[&(limit as i64) as &dyn rusqlite::ToSql],
            |row| {
                Ok(SecurityEvent {
                    timestamp: chrono::DateTime::from_timestamp(row.get::<_, i64>(0)?, 0)
                        .unwrap_or_default(),
                    event_type: parse_event_type(&row.get::<_, String>(1)?),
                    severity: parse_severity(&row.get::<_, String>(2)?),
                    client_id: row.get(3)?,
                    details: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    action_taken: row.get(5)?,
                })
            },
        )
    }

    /// 查询特定客户端的事件
    pub fn query_by_client(&self, client_id: &str, limit: usize) -> anyhow::Result<Vec<SecurityEvent>> {
        self.db.query_map(
            "SELECT timestamp, event_type, severity, client_id, details, action_taken
             FROM security_events WHERE client_id = ?1 ORDER BY timestamp DESC LIMIT ?2",
            &[&client_id as &dyn rusqlite::ToSql, &(limit as i64) as &dyn rusqlite::ToSql],
            |row| {
                Ok(SecurityEvent {
                    timestamp: chrono::DateTime::from_timestamp(row.get::<_, i64>(0)?, 0)
                        .unwrap_or_default(),
                    event_type: parse_event_type(&row.get::<_, String>(1)?),
                    severity: parse_severity(&row.get::<_, String>(2)?),
                    client_id: row.get(3)?,
                    details: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    action_taken: row.get(5)?,
                })
            },
        )
    }

    /// 统计事件数量
    pub fn count_events(&self) -> anyhow::Result<u64> {
        let counts = self.db.query_map(
            "SELECT COUNT(*) FROM security_events",
            &[],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(counts.first().copied().unwrap_or(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log() -> SecurityAuditLog {
        let path = crate::test_support::temp_db_path("security-audit");
        SecurityAuditLog::new(path).unwrap()
    }

    #[test]
    fn test_log_and_query() {
        let log = temp_log();

        let event = SecurityEvent {
            timestamp: Utc::now(),
            event_type: SecurityEventType::PromptInjectionAttempt,
            severity: Severity::High,
            client_id: Some("client1".into()),
            details: serde_json::json!({"input": "ignore previous instructions"}),
            action_taken: "blocked".into(),
        };

        log.log_event(event).unwrap();

        let events = log.query_recent(10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].client_id, Some("client1".into()));
    }

    #[test]
    fn test_query_by_client() {
        let log = temp_log();

        for i in 0..5 {
            let event = SecurityEvent {
                timestamp: Utc::now(),
                event_type: SecurityEventType::RateLimitExceeded,
                severity: Severity::Medium,
                client_id: Some(format!("client{}", i % 2)),
                details: serde_json::json!({}),
                action_taken: "throttled".into(),
            };
            log.log_event(event).unwrap();
        }

        let client0_events = log.query_by_client("client0", 10).unwrap();
        assert_eq!(client0_events.len(), 3);

        let client1_events = log.query_by_client("client1", 10).unwrap();
        assert_eq!(client1_events.len(), 2);
    }

    #[test]
    fn test_count_events() {
        let log = temp_log();

        assert_eq!(log.count_events().unwrap(), 0u64);

        for _ in 0..10 {
            let event = SecurityEvent {
                timestamp: Utc::now(),
                event_type: SecurityEventType::InvalidInput,
                severity: Severity::Low,
                client_id: None,
                details: serde_json::json!({}),
                action_taken: "rejected".into(),
            };
            log.log_event(event).unwrap();
        }

        assert_eq!(log.count_events().unwrap(), 10u64);
    }
}
