//! 数据库文件加密管理
//!
//! Phase 1 工程基线：提供对数据库文件的对称加密/解密与密钥轮换。

use anyhow::Context;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use std::path::Path;

const MAGIC: &[u8; 6] = b"PMENC1";
const NONCE_LEN: usize = 12;

/// 加密管理器
pub struct EncryptionManager {
    /// 原始密钥材料
    key: Vec<u8>,
}

impl EncryptionManager {
    /// 创建加密管理器
    pub fn new(key: Vec<u8>) -> Self {
        Self { key }
    }

    /// 从系统密钥链加载密钥（当前用环境变量替代）
    pub fn from_keychain(identifier: &str) -> anyhow::Result<Self> {
        let env_key = format!(
            "PROMETHEUS_KEYCHAIN_{}",
            identifier
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                })
                .collect::<String>()
        );

        let value = std::env::var(&env_key)
            .with_context(|| format!("missing keychain env var: {env_key}"))?;

        let key = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, value)
            .context("invalid base64 key material")?;
        Ok(Self::new(key))
    }

    /// 加密数据库文件（原地覆盖）
    pub fn encrypt_database(&self, db_path: &Path) -> anyhow::Result<()> {
        let plaintext = std::fs::read(db_path)
            .with_context(|| format!("failed to read db file: {}", db_path.display()))?;

        let key = self.aead_key()?;
        let nonce_bytes = random_nonce()?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = plaintext;
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| anyhow::anyhow!("failed to encrypt database content"))?;

        let mut payload = Vec::with_capacity(MAGIC.len() + NONCE_LEN + in_out.len());
        payload.extend_from_slice(MAGIC);
        payload.extend_from_slice(nonce_bytes.as_ref());
        payload.extend_from_slice(&in_out);

        atomic_write(db_path, &payload)
    }

    /// 解密数据库文件（原地覆盖）
    pub fn decrypt_database(&self, db_path: &Path) -> anyhow::Result<()> {
        let encrypted = std::fs::read(db_path)
            .with_context(|| format!("failed to read db file: {}", db_path.display()))?;

        if encrypted.len() < MAGIC.len() + NONCE_LEN {
            anyhow::bail!("encrypted payload too short");
        }
        if &encrypted[..MAGIC.len()] != MAGIC {
            anyhow::bail!("invalid encrypted payload magic");
        }

        let mut nonce_bytes = [0u8; NONCE_LEN];
        nonce_bytes.copy_from_slice(&encrypted[MAGIC.len()..MAGIC.len() + NONCE_LEN]);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let key = self.aead_key()?;
        let mut ciphertext = encrypted[MAGIC.len() + NONCE_LEN..].to_vec();
        let plaintext = key
            .open_in_place(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| anyhow::anyhow!("failed to decrypt database content"))?;

        atomic_write(db_path, plaintext)
    }

    /// 更改加密密钥
    pub fn rekey(&self, db_path: &Path, new_key: &[u8]) -> anyhow::Result<()> {
        self.decrypt_database(db_path)?;
        let next = EncryptionManager::new(new_key.to_vec());
        next.encrypt_database(db_path)
    }

    fn aead_key(&self) -> anyhow::Result<LessSafeKey> {
        let key_material = derive_key_material(&self.key);
        let unbound = UnboundKey::new(&CHACHA20_POLY1305, &key_material)
            .map_err(|_| anyhow::anyhow!("failed to initialize AEAD key"))?;
        Ok(LessSafeKey::new(unbound))
    }
}

fn derive_key_material(raw: &[u8]) -> [u8; 32] {
    let hash = digest(&SHA256, raw);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_ref());
    out
}

fn random_nonce() -> anyhow::Result<[u8; NONCE_LEN]> {
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| anyhow::anyhow!("failed to generate secure nonce"))?;
    Ok(nonce)
}

fn atomic_write(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp.enc");
    std::fs::write(&tmp, data)
        .with_context(|| format!("failed to write temp file: {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to replace target file: {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file() -> std::path::PathBuf {
        crate::test_support::temp_db_path("enc")
    }

    #[test]
    fn encrypt_and_decrypt_roundtrip() {
        let file = temp_file();
        std::fs::write(&file, b"sqlite-plain-data").unwrap();

        let manager = EncryptionManager::new(b"my-secret-key".to_vec());
        manager.encrypt_database(&file).unwrap();

        let encrypted = std::fs::read(&file).unwrap();
        assert_ne!(encrypted, b"sqlite-plain-data");
        assert_eq!(&encrypted[..MAGIC.len()], MAGIC);

        manager.decrypt_database(&file).unwrap();
        let decrypted = std::fs::read(&file).unwrap();
        assert_eq!(decrypted, b"sqlite-plain-data");
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let file = temp_file();
        std::fs::write(&file, b"sqlite-plain-data").unwrap();

        let manager_a = EncryptionManager::new(b"key-a".to_vec());
        manager_a.encrypt_database(&file).unwrap();

        let manager_b = EncryptionManager::new(b"key-b".to_vec());
        let result = manager_b.decrypt_database(&file);
        assert!(result.is_err());
    }

    #[test]
    fn rekey_allows_new_key_to_decrypt() {
        let file = temp_file();
        std::fs::write(&file, b"sqlite-plain-data").unwrap();

        let manager_a = EncryptionManager::new(b"key-a".to_vec());
        manager_a.encrypt_database(&file).unwrap();

        manager_a.rekey(&file, b"key-c").unwrap();

        let old_key = EncryptionManager::new(b"key-a".to_vec());
        assert!(old_key.decrypt_database(&file).is_err());

        let new_key = EncryptionManager::new(b"key-c".to_vec());
        new_key.decrypt_database(&file).unwrap();
        let decrypted = std::fs::read(&file).unwrap();
        assert_eq!(decrypted, b"sqlite-plain-data");
    }

    #[test]
    fn from_keychain_returns_error_when_env_missing() {
        let result = EncryptionManager::from_keychain("missing_key");
        assert!(result.is_err());
    }
}
