//! 密钥生命周期管理
//!
//! 软件 HSM 模式：密钥生成、轮换、销毁，使用 HKDF-SHA256 派生。

use chrono::{DateTime, Utc};
use ring::{hkdf, rand as ring_rand};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zeroize::Zeroize;

use crate::storage::db::Database;

/// 密钥用途分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyPurpose {
    Sanitization,
    AuditSigning,
    VaultExport,
}

impl std::fmt::Display for KeyPurpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sanitization => write!(f, "Sanitization"),
            Self::AuditSigning => write!(f, "AuditSigning"),
            Self::VaultExport => write!(f, "VaultExport"),
        }
    }
}

/// 密钥记录
#[derive(Debug, Clone)]
pub struct KeyRecord {
    pub key_id: String,
    pub purpose: String,
    pub created_at: DateTime<Utc>,
    pub active: bool,
}

/// 密钥生命周期管理器
pub struct KeyManager {
    db: Arc<Database>,
    /// 主密钥（用于加密存储的子密钥）
    master_key: [u8; 32],
}

impl KeyManager {
    /// 创建密钥管理器并初始化 schema
    pub fn new(db: Arc<Database>, master_key: [u8; 32]) -> anyhow::Result<Self> {
        let mgr = Self { db, master_key };
        mgr.init_schema()?;
        Ok(mgr)
    }

    fn init_schema(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS key_store (
                key_id TEXT PRIMARY KEY,
                purpose TEXT NOT NULL,
                encrypted_key BLOB NOT NULL,
                created_at_ms INTEGER NOT NULL,
                active INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_key_purpose ON key_store(purpose, active);",
        )
    }

    /// 生成新密钥（AES-256 随机密钥，用主密钥加密后存 SQLite）
    pub fn generate_key(&self, purpose: KeyPurpose) -> anyhow::Result<KeyRecord> {
        let key_id = uuid::Uuid::new_v4().to_string();
        let mut raw_key = [0u8; 32];
        ring_rand::SecureRandom::fill(&ring_rand::SystemRandom::new(), &mut raw_key)
            .map_err(|_| anyhow::anyhow!("failed to generate random key"))?;

        let encrypted = self.encrypt_with_master(&raw_key);
        raw_key.zeroize();

        let now = Utc::now();
        let now_ms = now.timestamp_millis();
        self.db.execute(
            "INSERT INTO key_store (key_id, purpose, encrypted_key, created_at_ms, active) VALUES (?1, ?2, ?3, ?4, 1)",
            &[&key_id as &dyn rusqlite::types::ToSql, &purpose.to_string(), &encrypted, &now_ms],
        )?;

        Ok(KeyRecord {
            key_id,
            purpose: purpose.to_string(),
            created_at: now,
            active: true,
        })
    }

    /// 获取指定用途的当前活跃密钥（解密后返回）
    pub fn get_active_key(&self, purpose: KeyPurpose) -> anyhow::Result<Option<(String, Vec<u8>)>> {
        let rows = self.db.query_map(
            "SELECT key_id, encrypted_key FROM key_store WHERE purpose = ?1 AND active = 1 ORDER BY created_at_ms DESC LIMIT 1",
            &[&purpose.to_string() as &dyn rusqlite::types::ToSql],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )?;

        match rows.into_iter().next() {
            Some((key_id, encrypted)) => {
                let decrypted = self.decrypt_with_master(&encrypted)?;
                Ok(Some((key_id, decrypted)))
            }
            None => Ok(None),
        }
    }

    /// 密钥轮换：生成新密钥，标记旧密钥为 inactive
    pub fn rotate_key(&self, purpose: KeyPurpose) -> anyhow::Result<KeyRecord> {
        // 停用当前活跃密钥
        self.db.execute(
            "UPDATE key_store SET active = 0 WHERE purpose = ?1 AND active = 1",
            &[&purpose.to_string() as &dyn rusqlite::types::ToSql],
        )?;
        // 生成新密钥
        self.generate_key(purpose)
    }

    /// 密钥销毁：zeroize + 从 DB 删除
    pub fn destroy_key(&self, key_id: &str) -> anyhow::Result<()> {
        self.db.execute(
            "DELETE FROM key_store WHERE key_id = ?1",
            &[&key_id as &dyn rusqlite::types::ToSql],
        )?;
        Ok(())
    }

    /// 使用 HKDF-SHA256 从主密钥派生子密钥
    pub fn derive_key(&self, info: &[u8], len: usize) -> anyhow::Result<Vec<u8>> {
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, &[0u8; 32]);
        let prk = salt.extract(&self.master_key);
        let info_refs = [info];
        let okm = prk
            .expand(&info_refs, HkdfLen(len))
            .map_err(|_| anyhow::anyhow!("HKDF expand failed"))?;
        let mut out = vec![0u8; len];
        okm.fill(&mut out)
            .map_err(|_| anyhow::anyhow!("HKDF fill failed"))?;
        Ok(out)
    }

    /// 列出指定用途的所有密钥记录
    pub fn list_keys(&self, purpose: KeyPurpose) -> anyhow::Result<Vec<KeyRecord>> {
        self.db.query_map(
            "SELECT key_id, purpose, created_at_ms, active FROM key_store WHERE purpose = ?1 ORDER BY created_at_ms DESC",
            &[&purpose.to_string() as &dyn rusqlite::types::ToSql],
            |row| {
                let ms: i64 = row.get(2)?;
                Ok(KeyRecord {
                    key_id: row.get(0)?,
                    purpose: row.get(1)?,
                    created_at: DateTime::from_timestamp_millis(ms).unwrap_or_default(),
                    active: row.get::<_, bool>(3)?,
                })
            },
        )
    }

    // ── 内部加密辅助 ──

    fn encrypt_with_master(&self, plaintext: &[u8]) -> Vec<u8> {
        use aes_gcm::aead::Aead;
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
        let cipher = Aes256Gcm::new_from_slice(&self.master_key).expect("invalid master key");
        let mut nonce_bytes = [0u8; 12];
        ring_rand::SecureRandom::fill(&ring_rand::SystemRandom::new(), &mut nonce_bytes)
            .expect("failed to generate nonce");
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, plaintext).expect("encryption failed");
        let mut out = nonce_bytes.to_vec();
        out.extend(ciphertext);
        out
    }

    fn decrypt_with_master(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        use aes_gcm::aead::Aead;
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
        if data.len() < 12 {
            anyhow::bail!("encrypted data too short");
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|_| anyhow::anyhow!("invalid master key"))?;
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow::anyhow!("decryption failed"))
    }
}

impl Drop for KeyManager {
    fn drop(&mut self) {
        self.master_key.zeroize();
    }
}

/// HKDF 输出长度适配器
struct HkdfLen(usize);

impl hkdf::KeyType for HkdfLen {
    fn len(&self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        let path = crate::test_support::temp_db_path("key-mgr");
        Arc::new(Database::open(path).unwrap())
    }

    fn test_master_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        ring_rand::SecureRandom::fill(&ring_rand::SystemRandom::new(), &mut key).unwrap();
        key
    }

    #[test]
    fn generate_and_retrieve_key() {
        let db = test_db();
        let mgr = KeyManager::new(db, test_master_key()).unwrap();
        mgr.generate_key(KeyPurpose::Sanitization).unwrap();

        let (key_id, raw) = mgr
            .get_active_key(KeyPurpose::Sanitization)
            .unwrap()
            .unwrap();
        assert!(!key_id.is_empty());
        assert_eq!(raw.len(), 32);
    }

    #[test]
    fn rotate_key_deactivates_old() {
        let db = test_db();
        let mgr = KeyManager::new(db, test_master_key()).unwrap();
        let old = mgr.generate_key(KeyPurpose::AuditSigning).unwrap();
        let new = mgr.rotate_key(KeyPurpose::AuditSigning).unwrap();

        assert_ne!(old.key_id, new.key_id);
        let keys = mgr.list_keys(KeyPurpose::AuditSigning).unwrap();
        let active_count = keys.iter().filter(|k| k.active).count();
        assert_eq!(active_count, 1);
    }

    #[test]
    fn destroy_key_removes_from_db() {
        let db = test_db();
        let mgr = KeyManager::new(db, test_master_key()).unwrap();
        let rec = mgr.generate_key(KeyPurpose::VaultExport).unwrap();
        mgr.destroy_key(&rec.key_id).unwrap();

        let result = mgr.get_active_key(KeyPurpose::VaultExport).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn derive_key_deterministic() {
        let db = test_db();
        let master = test_master_key();
        let mgr = KeyManager::new(db, master).unwrap();
        let k1 = mgr.derive_key(b"audit-signing", 32).unwrap();
        let k2 = mgr.derive_key(b"audit-signing", 32).unwrap();
        assert_eq!(k1, k2);
        let k3 = mgr.derive_key(b"different-purpose", 32).unwrap();
        assert_ne!(k1, k3);
    }
}
