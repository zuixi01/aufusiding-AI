//! Token 加密持久化
//!
//! 使用 AES-256-GCM 加密 TokenStore，按 device_id 存储到磁盘。
//! 进程重启后自动恢复 Token。

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, KeyInit};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::oauth::TokenStore;

/// 磁盘上的加密 Token 条目
#[derive(Debug, Serialize, Deserialize)]
struct EncryptedEntry {
    /// 12 字节 nonce (base64)
    nonce: String,
    /// 加密后的 TokenStore JSON (base64)
    ciphertext: String,
}

/// Token 持久化文件格式
#[derive(Debug, Serialize, Deserialize, Default)]
struct TokenFile {
    /// device_id → EncryptedEntry
    tokens: HashMap<String, EncryptedEntry>,
}

/// Token 持久化管理器
pub struct TokenPersistence {
    cipher: Aes256Gcm,
    file_path: PathBuf,
}

impl TokenPersistence {
    /// 创建持久化管理器
    ///
    /// - `master_key`: 32 字节 AES-256 密钥
    /// - `file_path`: Token 存储文件路径
    pub fn new(master_key: &[u8; 32], file_path: impl Into<PathBuf>) -> Self {
        let cipher = Aes256Gcm::new_from_slice(master_key).expect("invalid key length");
        Self {
            cipher,
            file_path: file_path.into(),
        }
    }
    /// 保存 Token（加密后写入磁盘）
    pub fn save(&self, device_id: &str, token: &TokenStore) -> anyhow::Result<()> {
        let mut file = self.load_file()?;

        let plaintext = serde_json::to_vec(token).context("failed to serialize token")?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_ref())
            .map_err(|e| anyhow::anyhow!("encryption failed: {}", e))?;

        file.tokens.insert(
            device_id.to_string(),
            EncryptedEntry {
                nonce: b64_encode(nonce.as_slice()),
                ciphertext: b64_encode(&ciphertext),
            },
        );

        self.write_file(&file)
    }

    /// 加载 Token（从磁盘读取并解密）
    pub fn load(&self, device_id: &str) -> anyhow::Result<Option<TokenStore>> {
        let file = self.load_file()?;

        let entry = match file.tokens.get(device_id) {
            Some(e) => e,
            None => return Ok(None),
        };

        let nonce_bytes = b64_decode(&entry.nonce).context("invalid nonce")?;
        let ciphertext = b64_decode(&entry.ciphertext).context("invalid ciphertext")?;

        let nonce = aes_gcm::Nonce::from_exact_iter(nonce_bytes.into_iter())
            .ok_or_else(|| anyhow::anyhow!("nonce must be 12 bytes"))?;

        let plaintext = self
            .cipher
            .decrypt(&nonce, ciphertext.as_ref())
            .map_err(|e| anyhow::anyhow!("decryption failed: {}", e))?;

        let token: TokenStore =
            serde_json::from_slice(&plaintext).context("failed to deserialize token")?;
        Ok(Some(token))
    }

    /// 删除 Token
    pub fn delete(&self, device_id: &str) -> anyhow::Result<bool> {
        let mut file = self.load_file()?;
        let removed = file.tokens.remove(device_id).is_some();
        if removed {
            self.write_file(&file)?;
        }
        Ok(removed)
    }

    /// 列出所有已存储的 device_id
    pub fn list_devices(&self) -> anyhow::Result<Vec<String>> {
        let file = self.load_file()?;
        Ok(file.tokens.keys().cloned().collect())
    }

    /// 加载所有 Token（批量解密）
    pub fn load_all(&self) -> anyhow::Result<HashMap<String, TokenStore>> {
        let file = self.load_file()?;
        let mut result = HashMap::new();
        for device_id in file.tokens.keys() {
            if let Some(token) = self.load(device_id)? {
                result.insert(device_id.clone(), token);
            }
        }
        Ok(result)
    }

    // ── 内部方法 ────────────────────────────────────────────────────

    fn load_file(&self) -> anyhow::Result<TokenFile> {
        if !self.file_path.exists() {
            return Ok(TokenFile::default());
        }
        let data = std::fs::read_to_string(&self.file_path).context("failed to read token file")?;
        serde_json::from_str(&data).context("failed to parse token file")
    }

    fn write_file(&self, file: &TokenFile) -> anyhow::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).context("failed to create token directory")?;
        }
        let data = serde_json::to_string_pretty(file).context("failed to serialize token file")?;
        std::fs::write(&self.file_path, data).context("failed to write token file")
    }
}

fn b64_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode(data)
}

fn b64_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.decode(s).context("base64 decode failed")
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use tempfile::TempDir;

    fn test_key() -> [u8; 32] {
        [0xAB; 32]
    }

    fn make_token(access: &str) -> TokenStore {
        TokenStore {
            access_token: access.into(),
            refresh_token: Some("refresh-tok".into()),
            expires_at: Some(Utc::now() + Duration::hours(1)),
            token_type: "Bearer".into(),
            scope: Some("read write".into()),
        }
    }

    fn setup() -> (TokenPersistence, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tokens.json");
        let store = TokenPersistence::new(&test_key(), path);
        (store, dir)
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (store, _dir) = setup();
        let token = make_token("access-123");
        store.save("tesla-1", &token).unwrap();

        let loaded = store.load("tesla-1").unwrap().unwrap();
        assert_eq!(loaded.access_token, "access-123");
        assert_eq!(loaded.refresh_token, Some("refresh-tok".into()));
        assert_eq!(loaded.token_type, "Bearer");
        assert_eq!(loaded.scope, Some("read write".into()));
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let (store, _dir) = setup();
        assert!(store.load("no-such-device").unwrap().is_none());
    }

    #[test]
    fn save_overwrites_existing() {
        let (store, _dir) = setup();
        store.save("dev-1", &make_token("old")).unwrap();
        store.save("dev-1", &make_token("new")).unwrap();

        let loaded = store.load("dev-1").unwrap().unwrap();
        assert_eq!(loaded.access_token, "new");
    }

    #[test]
    fn multiple_devices() {
        let (store, _dir) = setup();
        store.save("tesla", &make_token("t-access")).unwrap();
        store.save("bmw", &make_token("b-access")).unwrap();
        store.save("obd", &make_token("o-access")).unwrap();

        assert_eq!(
            store.load("tesla").unwrap().unwrap().access_token,
            "t-access"
        );
        assert_eq!(store.load("bmw").unwrap().unwrap().access_token, "b-access");
        assert_eq!(store.load("obd").unwrap().unwrap().access_token, "o-access");
    }

    #[test]
    fn delete_removes_token() {
        let (store, _dir) = setup();
        store.save("dev-1", &make_token("a")).unwrap();
        assert!(store.delete("dev-1").unwrap());
        assert!(store.load("dev-1").unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let (store, _dir) = setup();
        assert!(!store.delete("no-such").unwrap());
    }

    #[test]
    fn list_devices_returns_all_ids() {
        let (store, _dir) = setup();
        store.save("a", &make_token("1")).unwrap();
        store.save("b", &make_token("2")).unwrap();

        let mut devices = store.list_devices().unwrap();
        devices.sort();
        assert_eq!(devices, vec!["a", "b"]);
    }

    #[test]
    fn load_all_returns_all_tokens() {
        let (store, _dir) = setup();
        store.save("x", &make_token("x-tok")).unwrap();
        store.save("y", &make_token("y-tok")).unwrap();

        let all = store.load_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["x"].access_token, "x-tok");
        assert_eq!(all["y"].access_token, "y-tok");
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tokens.json");

        let store1 = TokenPersistence::new(&[0xAB; 32], &path);
        store1.save("dev", &make_token("secret")).unwrap();

        let store2 = TokenPersistence::new(&[0xCD; 32], &path);
        let result = store2.load("dev");
        assert!(result.is_err());
    }

    #[test]
    fn file_persists_across_instances() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tokens.json");
        let key = test_key();

        {
            let store = TokenPersistence::new(&key, &path);
            store.save("persistent", &make_token("survives")).unwrap();
        }

        // 新实例，同一文件
        let store2 = TokenPersistence::new(&key, &path);
        let loaded = store2.load("persistent").unwrap().unwrap();
        assert_eq!(loaded.access_token, "survives");
    }

    #[test]
    fn encryption_produces_different_ciphertext() {
        let (store, _dir) = setup();
        let token = make_token("same-token");
        store.save("dev-a", &token).unwrap();
        store.save("dev-b", &token).unwrap();

        let file = store.load_file().unwrap();
        let a = &file.tokens["dev-a"];
        let b = &file.tokens["dev-b"];
        // 不同 nonce → 不同密文
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[test]
    fn empty_file_returns_empty() {
        let (store, _dir) = setup();
        let devices = store.list_devices().unwrap();
        assert!(devices.is_empty());
        let all = store.load_all().unwrap();
        assert!(all.is_empty());
    }
}
