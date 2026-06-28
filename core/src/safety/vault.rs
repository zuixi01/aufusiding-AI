//! 安全金库
//!
//! 加密存储敏感信息（密码、API 密钥、私钥等），支持生物识别解锁。

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 金库条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    /// 条目 ID
    pub id: Uuid,
    /// 条目名称
    pub name: String,
    /// 加密后的数据（nonce 12 bytes || ciphertext）
    pub encrypted_data: Vec<u8>,
    /// 条目类型（密码、API 密钥等）
    pub entry_type: String,
}

/// 安全金库
pub struct SecureVault {
    /// 主密钥（32 bytes for AES-256）
    master_key: Vec<u8>,
    /// 内存存储
    entries: HashMap<Uuid, VaultEntry>,
}

impl SecureVault {
    /// 创建安全金库
    pub fn new(master_key: Vec<u8>) -> Self {
        Self {
            master_key,
            entries: HashMap::new(),
        }
    }

    fn cipher(&self) -> anyhow::Result<Aes256Gcm> {
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&self.master_key);
        Ok(Aes256Gcm::new(key))
    }

    /// 存储敏感数据
    pub async fn store(
        &mut self,
        name: &str,
        data: &[u8],
        entry_type: &str,
    ) -> anyhow::Result<Uuid> {
        let cipher = self.cipher()?;
        let nonce_bytes = aes_gcm::aead::rand_core::RngCore::next_u64(&mut OsRng);
        let mut nonce_buf = [0u8; 12];
        nonce_buf[..8].copy_from_slice(&nonce_bytes.to_le_bytes());
        let nonce = Nonce::from_slice(&nonce_buf);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

        let mut encrypted_data = nonce_buf.to_vec();
        encrypted_data.extend_from_slice(&ciphertext);

        let id = Uuid::new_v4();
        let entry = VaultEntry {
            id,
            name: name.to_string(),
            encrypted_data,
            entry_type: entry_type.to_string(),
        };
        self.entries.insert(id, entry);
        Ok(id)
    }

    /// 检索敏感数据
    pub async fn retrieve(&self, id: Uuid) -> anyhow::Result<Vec<u8>> {
        let entry = self
            .entries
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("entry not found: {id}"))?;

        let cipher = self.cipher()?;
        let (nonce_bytes, ciphertext) = entry.encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))
    }

    /// 删除条目
    pub async fn delete(&mut self, id: Uuid) -> anyhow::Result<()> {
        self.entries
            .remove(&id)
            .ok_or_else(|| anyhow::anyhow!("entry not found: {id}"))?;
        Ok(())
    }

    /// 列出所有条目（不包含数据）
    pub async fn list(&self) -> anyhow::Result<Vec<VaultEntry>> {
        Ok(self.entries.values().cloned().collect())
    }

    /// 使用生物识别解锁（需平台 API 集成：iOS FaceID/TouchID, Android BiometricPrompt）
    pub async fn unlock_with_biometric(&self) -> anyhow::Result<()> {
        anyhow::bail!("biometric unlock requires platform-specific integration (iOS/Android)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Vec<u8> {
        vec![0xAB; 32]
    }

    #[tokio::test]
    async fn new_creates_vault_with_key() {
        let vault = SecureVault::new(test_key());
        assert!(vault.entries.is_empty());
        assert_eq!(vault.master_key.len(), 32);
    }

    #[tokio::test]
    async fn store_returns_unique_id() {
        let mut vault = SecureVault::new(test_key());
        let id1 = vault.store("a", b"data1", "password").await.unwrap();
        let id2 = vault.store("b", b"data2", "password").await.unwrap();
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn store_and_retrieve_roundtrip() {
        let mut vault = SecureVault::new(test_key());
        let data = b"super-secret-api-key";
        let id = vault.store("my-key", data, "api_key").await.unwrap();
        let retrieved = vault.retrieve(id).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn retrieve_nonexistent_returns_error() {
        let vault = SecureVault::new(test_key());
        let err = vault.retrieve(Uuid::new_v4()).await.unwrap_err();
        assert!(err.to_string().contains("entry not found"));
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let mut vault = SecureVault::new(test_key());
        let id = vault.store("x", b"data", "password").await.unwrap();
        vault.delete(id).await.unwrap();
        assert!(vault.retrieve(id).await.is_err());
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_error() {
        let mut vault = SecureVault::new(test_key());
        let err = vault.delete(Uuid::new_v4()).await.unwrap_err();
        assert!(err.to_string().contains("entry not found"));
    }

    #[tokio::test]
    async fn list_returns_all_entries() {
        let mut vault = SecureVault::new(test_key());
        vault.store("a", b"1", "pw").await.unwrap();
        vault.store("b", b"2", "pw").await.unwrap();
        vault.store("c", b"3", "pw").await.unwrap();
        let entries = vault.list().await.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn list_empty_vault() {
        let vault = SecureVault::new(test_key());
        let entries = vault.list().await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn store_encrypts_data() {
        let mut vault = SecureVault::new(test_key());
        let plain = b"plaintext-secret";
        let id = vault.store("enc", plain, "pw").await.unwrap();
        let entry = vault.entries.get(&id).unwrap();
        // encrypted_data = 12-byte nonce + ciphertext, should differ from plain
        assert_ne!(&entry.encrypted_data[12..], plain);
    }

    #[tokio::test]
    async fn different_keys_produce_different_ciphertext() {
        let data = b"same-data";
        let mut vault1 = SecureVault::new(vec![0x01; 32]);
        let mut vault2 = SecureVault::new(vec![0x02; 32]);
        let id1 = vault1.store("a", data, "pw").await.unwrap();
        let id2 = vault2.store("a", data, "pw").await.unwrap();
        let ct1 = &vault1.entries.get(&id1).unwrap().encrypted_data[12..];
        let ct2 = &vault2.entries.get(&id2).unwrap().encrypted_data[12..];
        assert_ne!(ct1, ct2);
    }

    #[tokio::test]
    async fn wrong_key_fails_decryption() {
        let mut vault = SecureVault::new(vec![0x01; 32]);
        let id = vault.store("s", b"secret", "pw").await.unwrap();
        // swap key
        vault.master_key = vec![0x02; 32];
        let err = vault.retrieve(id).await.unwrap_err();
        assert!(err.to_string().contains("decryption failed"));
    }

    #[tokio::test]
    async fn store_multiple_entries() {
        let mut vault = SecureVault::new(test_key());
        for i in 0..10 {
            vault
                .store(&format!("entry-{i}"), format!("data-{i}").as_bytes(), "pw")
                .await
                .unwrap();
        }
        assert_eq!(vault.entries.len(), 10);
        let entries = vault.list().await.unwrap();
        assert_eq!(entries.len(), 10);
    }
}
