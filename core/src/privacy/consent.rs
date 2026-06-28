//! 同意管理
//!
//! 记录、查询、撤销用户的数据处理同意，提供审计日志。

use crate::storage::Database;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 同意目的
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConsentPurpose {
    /// 数据收集
    DataCollection,
    /// 数据分析
    Analytics,
    /// 个性化推荐
    Personalization,
    /// 第三方共享
    ThirdPartySharing,
    /// 语音数据处理
    VoiceProcessing,
}

impl ConsentPurpose {
    fn as_str(&self) -> &'static str {
        match self {
            Self::DataCollection => "data_collection",
            Self::Analytics => "analytics",
            Self::Personalization => "personalization",
            Self::ThirdPartySharing => "third_party_sharing",
            Self::VoiceProcessing => "voice_processing",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "data_collection" => Some(Self::DataCollection),
            "analytics" => Some(Self::Analytics),
            "personalization" => Some(Self::Personalization),
            "third_party_sharing" => Some(Self::ThirdPartySharing),
            "voice_processing" => Some(Self::VoiceProcessing),
            _ => None,
        }
    }
}

/// 同意记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub user_id: String,
    pub purpose: ConsentPurpose,
    pub granted: bool,
    pub timestamp: DateTime<Utc>,
}

/// 同意管理器
pub struct ConsentManager {
    db: Database,
}

impl ConsentManager {
    /// 创建同意管理器
    pub fn new(db_path: PathBuf) -> anyhow::Result<Self> {
        let db = Database::open(db_path)?;
        let mgr = Self { db };
        mgr.init_schema()?;
        Ok(mgr)
    }

    fn init_schema(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS consent_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id TEXT NOT NULL,
                purpose TEXT NOT NULL,
                granted INTEGER NOT NULL,
                timestamp_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_consent_user
              ON consent_records(user_id, purpose);",
        )
    }

    /// 记录同意
    pub fn record_consent(
        &self,
        user_id: &str,
        purpose: ConsentPurpose,
        granted: bool,
    ) -> anyhow::Result<()> {
        let ts = Utc::now().timestamp_millis();
        self.db.execute(
            "INSERT INTO consent_records (user_id, purpose, granted, timestamp_ms)
             VALUES (?1, ?2, ?3, ?4)",
            &[
                &user_id.to_string(),
                &purpose.as_str().to_string(),
                &(granted as i64),
                &ts,
            ],
        )?;
        Ok(())
    }

    /// 检查同意状态（取最新记录）
    pub fn check_consent(&self, user_id: &str, purpose: ConsentPurpose) -> bool {
        let purpose_str = purpose.as_str().to_string();
        let results = self.db.query_map(
            "SELECT granted FROM consent_records
             WHERE user_id = ?1 AND purpose = ?2
             ORDER BY timestamp_ms DESC, id DESC LIMIT 1",
            &[&user_id.to_string(), &purpose_str],
            |row| row.get::<_, i64>(0),
        );
        match results {
            Ok(rows) => rows.first().is_some_and(|&v| v != 0),
            Err(_) => false,
        }
    }

    /// 撤销同意
    pub fn revoke_consent(&self, user_id: &str, purpose: ConsentPurpose) -> anyhow::Result<()> {
        self.record_consent(user_id, purpose, false)
    }

    /// 获取审计日志
    pub fn get_audit_log(&self, user_id: &str) -> anyhow::Result<Vec<ConsentRecord>> {
        self.db.query_map(
            "SELECT user_id, purpose, granted, timestamp_ms
             FROM consent_records
             WHERE user_id = ?1
             ORDER BY timestamp_ms ASC, id ASC",
            &[&user_id.to_string()],
            |row| {
                let uid: String = row.get(0)?;
                let purpose_str: String = row.get(1)?;
                let granted: i64 = row.get(2)?;
                let ts_ms: i64 = row.get(3)?;
                Ok(ConsentRecord {
                    user_id: uid,
                    purpose: ConsentPurpose::from_str(&purpose_str)
                        .unwrap_or(ConsentPurpose::DataCollection),
                    granted: granted != 0,
                    timestamp: Utc
                        .timestamp_millis_opt(ts_ms)
                        .single()
                        .unwrap_or_else(Utc::now),
                })
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager() -> ConsentManager {
        let path = crate::test_support::temp_db_path("consent-test");
        ConsentManager::new(path).unwrap()
    }

    #[test]
    fn record_and_check_consent() {
        let mgr = test_manager();
        mgr.record_consent("user1", ConsentPurpose::Analytics, true)
            .unwrap();
        assert!(mgr.check_consent("user1", ConsentPurpose::Analytics));
    }

    #[test]
    fn no_consent_returns_false() {
        let mgr = test_manager();
        assert!(!mgr.check_consent("user1", ConsentPurpose::Analytics));
    }

    #[test]
    fn revoke_consent() {
        let mgr = test_manager();
        mgr.record_consent("user1", ConsentPurpose::DataCollection, true)
            .unwrap();
        assert!(mgr.check_consent("user1", ConsentPurpose::DataCollection));

        mgr.revoke_consent("user1", ConsentPurpose::DataCollection)
            .unwrap();
        assert!(!mgr.check_consent("user1", ConsentPurpose::DataCollection));
    }

    #[test]
    fn audit_log_ordered() {
        let mgr = test_manager();
        mgr.record_consent("user1", ConsentPurpose::Analytics, true)
            .unwrap();
        mgr.record_consent("user1", ConsentPurpose::Personalization, false)
            .unwrap();
        mgr.revoke_consent("user1", ConsentPurpose::Analytics)
            .unwrap();

        let log = mgr.get_audit_log("user1").unwrap();
        assert_eq!(log.len(), 3);
        assert!(log[0].granted);
        assert!(!log[1].granted);
        assert!(!log[2].granted);
    }

    #[test]
    fn separate_users() {
        let mgr = test_manager();
        mgr.record_consent("user1", ConsentPurpose::Analytics, true)
            .unwrap();
        mgr.record_consent("user2", ConsentPurpose::Analytics, false)
            .unwrap();

        assert!(mgr.check_consent("user1", ConsentPurpose::Analytics));
        assert!(!mgr.check_consent("user2", ConsentPurpose::Analytics));
    }

    #[test]
    fn separate_purposes() {
        let mgr = test_manager();
        mgr.record_consent("user1", ConsentPurpose::Analytics, true)
            .unwrap();
        assert!(!mgr.check_consent("user1", ConsentPurpose::VoiceProcessing));
    }

    #[test]
    fn consent_purpose_roundtrip() {
        let purposes = vec![
            ConsentPurpose::DataCollection,
            ConsentPurpose::Analytics,
            ConsentPurpose::Personalization,
            ConsentPurpose::ThirdPartySharing,
            ConsentPurpose::VoiceProcessing,
        ];
        for p in purposes {
            let s = p.as_str();
            assert_eq!(ConsentPurpose::from_str(s), Some(p));
        }
    }

    #[test]
    fn empty_audit_log() {
        let mgr = test_manager();
        let log = mgr.get_audit_log("nobody").unwrap();
        assert!(log.is_empty());
    }
}
