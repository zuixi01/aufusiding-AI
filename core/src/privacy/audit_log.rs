//! 防篡改审计日志
//!
//! SQLite + 哈希链 + HMAC 签名，每条记录包含前一条的哈希，形成链式结构。

use chrono::{DateTime, Utc};
use ring::{digest, hmac};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::storage::db::Database;

/// 审计操作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOperation {
    Sanitize,
    Reverse,
    Destroy,
    KeyRotate,
    TtlExpire,
}

impl std::fmt::Display for AuditOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sanitize => write!(f, "Sanitize"),
            Self::Reverse => write!(f, "Reverse"),
            Self::Destroy => write!(f, "Destroy"),
            Self::KeyRotate => write!(f, "KeyRotate"),
            Self::TtlExpire => write!(f, "TtlExpire"),
        }
    }
}

/// 审计日志条目
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub operation: String,
    pub entity_type: String,
    pub strategy_used: String,
    pub sensitivity_level: u8,
    /// SHA-256(原始数据)，不存原文
    pub data_hash: String,
    /// SHA-256(prev_hash || entry_bytes) 链式哈希
    pub chain_hash: String,
    /// HMAC-SHA256 签名（hex）
    pub signature: String,
}

/// 防篡改审计日志
pub struct SanitizationAuditLog {
    db: Arc<Database>,
    signing_key: Vec<u8>,
}

impl SanitizationAuditLog {
    /// 创建审计日志并初始化 schema
    pub fn new(db: Arc<Database>, signing_key: Vec<u8>) -> anyhow::Result<Self> {
        let log = Self { db, signing_key };
        log.init_schema()?;
        Ok(log)
    }

    fn init_schema(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS sanitization_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                operation TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                strategy_used TEXT NOT NULL,
                sensitivity_level INTEGER NOT NULL,
                data_hash TEXT NOT NULL,
                chain_hash TEXT NOT NULL,
                signature TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_audit_ts ON sanitization_audit(timestamp_ms);",
        )
    }

    /// 记录一条审计日志
    pub fn log_operation(
        &self,
        operation: AuditOperation,
        entity_type: &str,
        strategy_used: &str,
        sensitivity_level: u8,
        original_data: &str,
    ) -> anyhow::Result<i64> {
        let now = Utc::now();
        let now_ms = now.timestamp_millis();

        // SHA-256(原始数据)
        let data_hash = sha256_hex(original_data.as_bytes());

        // 获取前一条记录的 chain_hash
        let prev_hash = self
            .get_last_chain_hash()?
            .unwrap_or_else(|| "GENESIS".to_string());

        // 构造 entry 字节用于链式哈希
        let entry_bytes = format!(
            "{}|{}|{}|{}|{}|{}",
            now_ms, operation, entity_type, strategy_used, sensitivity_level, data_hash
        );
        let chain_input = format!("{}{}", prev_hash, entry_bytes);
        let chain_hash = sha256_hex(chain_input.as_bytes());

        // HMAC-SHA256 签名
        let sign_payload = format!("{}|{}", chain_hash, entry_bytes);
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.signing_key);
        let tag = hmac::sign(&key, sign_payload.as_bytes());
        let signature = hex_encode(tag.as_ref());

        self.db.execute(
            "INSERT INTO sanitization_audit (timestamp_ms, operation, entity_type, strategy_used, sensitivity_level, data_hash, chain_hash, signature) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[
                &now_ms as &dyn rusqlite::types::ToSql,
                &operation.to_string(),
                &entity_type.to_string(),
                &strategy_used.to_string(),
                &(sensitivity_level as i64),
                &data_hash,
                &chain_hash,
                &signature,
            ],
        )?;

        // 返回插入的 id
        let ids = self
            .db
            .query_map("SELECT last_insert_rowid()", &[], |row| {
                row.get::<_, i64>(0)
            })?;
        Ok(ids.into_iter().next().unwrap_or(0))
    }

    /// 验证整条审计链的完整性
    pub fn verify_chain(&self) -> anyhow::Result<bool> {
        let entries = self.get_all_entries()?;
        if entries.is_empty() {
            return Ok(true);
        }

        let mut prev_hash = "GENESIS".to_string();
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.signing_key);

        for entry in &entries {
            // 重建 entry_bytes
            let ts_ms = entry.timestamp.timestamp_millis();
            let entry_bytes = format!(
                "{}|{}|{}|{}|{}|{}",
                ts_ms,
                entry.operation,
                entry.entity_type,
                entry.strategy_used,
                entry.sensitivity_level,
                entry.data_hash
            );

            // 验证链式哈希
            let chain_input = format!("{}{}", prev_hash, entry_bytes);
            let expected_chain = sha256_hex(chain_input.as_bytes());
            if expected_chain != entry.chain_hash {
                return Ok(false);
            }

            // 验证 HMAC 签名
            let sign_payload = format!("{}|{}", entry.chain_hash, entry_bytes);
            let expected_tag = hmac::sign(&key, sign_payload.as_bytes());
            if hex_encode(expected_tag.as_ref()) != entry.signature {
                return Ok(false);
            }

            prev_hash = entry.chain_hash.clone();
        }

        Ok(true)
    }

    /// 获取所有审计条目
    pub fn get_all_entries(&self) -> anyhow::Result<Vec<AuditEntry>> {
        self.db.query_map(
            "SELECT id, timestamp_ms, operation, entity_type, strategy_used, sensitivity_level, data_hash, chain_hash, signature FROM sanitization_audit ORDER BY id ASC",
            &[],
            |row| {
                let ms: i64 = row.get(1)?;
                Ok(AuditEntry {
                    id: row.get(0)?,
                    timestamp: DateTime::from_timestamp_millis(ms).unwrap_or_default(),
                    operation: row.get(2)?,
                    entity_type: row.get(3)?,
                    strategy_used: row.get(4)?,
                    sensitivity_level: row.get::<_, i64>(5)? as u8,
                    data_hash: row.get(6)?,
                    chain_hash: row.get(7)?,
                    signature: row.get(8)?,
                })
            },
        )
    }

    /// 获取审计条目数量
    pub fn entry_count(&self) -> anyhow::Result<usize> {
        let counts = self
            .db
            .query_map("SELECT COUNT(*) FROM sanitization_audit", &[], |row| {
                row.get::<_, i64>(0)
            })?;
        Ok(counts.into_iter().next().unwrap_or(0) as usize)
    }

    fn get_last_chain_hash(&self) -> anyhow::Result<Option<String>> {
        let rows = self.db.query_map(
            "SELECT chain_hash FROM sanitization_audit ORDER BY id DESC LIMIT 1",
            &[],
            |row| row.get::<_, String>(0),
        )?;
        Ok(rows.into_iter().next())
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let d = digest::digest(&digest::SHA256, data);
    hex_encode(d.as_ref())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        let path = crate::test_support::temp_db_path("audit-log");
        Arc::new(Database::open(path).unwrap())
    }

    #[test]
    fn log_and_verify_chain() {
        let db = test_db();
        let log =
            SanitizationAuditLog::new(db, b"test-signing-key-32bytes!!!!!!!!".to_vec()).unwrap();

        log.log_operation(
            AuditOperation::Sanitize,
            "EMAIL",
            "Tokenize",
            1,
            "test@mail.com",
        )
        .unwrap();
        log.log_operation(
            AuditOperation::Sanitize,
            "PHONE",
            "Redact",
            3,
            "13812345678",
        )
        .unwrap();

        assert_eq!(log.entry_count().unwrap(), 2);
        assert!(log.verify_chain().unwrap());
    }

    #[test]
    fn tampered_entry_detected() {
        let db = test_db();
        let log =
            SanitizationAuditLog::new(db.clone(), b"test-signing-key-32bytes!!!!!!!!".to_vec())
                .unwrap();

        log.log_operation(AuditOperation::Sanitize, "EMAIL", "Tokenize", 1, "a@b.com")
            .unwrap();
        log.log_operation(AuditOperation::Destroy, "PHONE", "Redact", 2, "13800000000")
            .unwrap();

        // 篡改第一条记录的 data_hash
        db.execute(
            "UPDATE sanitization_audit SET data_hash = 'tampered' WHERE id = 1",
            &[],
        )
        .unwrap();

        assert!(!log.verify_chain().unwrap());
    }

    #[test]
    fn empty_chain_is_valid() {
        let db = test_db();
        let log =
            SanitizationAuditLog::new(db, b"key-for-empty-chain-test!!!!!!!!".to_vec()).unwrap();
        assert!(log.verify_chain().unwrap());
    }

    #[test]
    fn entries_ordered_by_id() {
        let db = test_db();
        let log =
            SanitizationAuditLog::new(db, b"key-for-ordering-test!!!!!!!!!!!".to_vec()).unwrap();

        log.log_operation(AuditOperation::Sanitize, "EMAIL", "Tokenize", 1, "first")
            .unwrap();
        log.log_operation(AuditOperation::KeyRotate, "SYSTEM", "N/A", 0, "rotate")
            .unwrap();
        log.log_operation(AuditOperation::TtlExpire, "PHONE", "Tokenize", 1, "expire")
            .unwrap();

        let entries = log.get_all_entries().unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].id < entries[1].id);
        assert!(entries[1].id < entries[2].id);
    }
}
