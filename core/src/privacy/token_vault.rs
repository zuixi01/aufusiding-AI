//! 可逆 Token 映射表
//!
//! 将 PII 原文映射为不可逆 token，支持会话内去重和安全擦除。

use std::collections::HashMap;
use std::time::{Duration, Instant};
use zeroize::Zeroize;

use super::destruction_proof::DestructionProof;
use super::pii_detector::PiiEntity;

/// Vault 统计信息
#[derive(Debug, Clone)]
pub struct VaultStats {
    pub total_tokens: usize,
    pub by_type: HashMap<String, u32>,
    pub expired_count: usize,
}

/// 单条映射条目（含创建时间）
struct VaultEntry {
    original: String,
    created_at: Instant,
}

/// 可逆 token 映射表
pub struct TokenVault {
    forward: HashMap<String, String>,
    reverse: HashMap<String, VaultEntry>,
    cipher_key: [u8; 32],
    counters: HashMap<String, u32>,
    /// TTL 配置（None = 永不过期，向后兼容）
    ttl: Option<Duration>,
    /// 销毁证明签名密钥
    signing_key: Vec<u8>,
}

impl TokenVault {
    pub fn new() -> Self {
        let mut key = [0u8; 32];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut key)
            .expect("failed to generate random key");
        // 默认签名密钥从 cipher_key 派生
        let signing_key = key.to_vec();
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
            cipher_key: key,
            counters: HashMap::new(),
            ttl: None,
            signing_key,
        }
    }

    /// 带 TTL 的构造器
    pub fn new_with_ttl(ttl: Duration) -> Self {
        let mut vault = Self::new();
        vault.ttl = Some(ttl);
        vault
    }

    /// 将 PII 实体替换为 token，同一原文映射到同一 token
    pub fn tokenize(&mut self, entity: &PiiEntity) -> String {
        if let Some(existing) = self.forward.get(&entity.text) {
            return existing.clone();
        }
        let type_name = entity.entity_type.to_string();
        let counter = self.counters.entry(type_name.clone()).or_insert(0);
        *counter += 1;
        let mut rand_bytes = [0u8; 2];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut rand_bytes)
            .expect("failed to generate random bytes");
        let hex = format!("{:02x}{:02x}", rand_bytes[0], rand_bytes[1]);
        let token = format!("[{}_{hex}]", type_name);
        self.forward.insert(entity.text.clone(), token.clone());
        self.reverse.insert(
            token.clone(),
            VaultEntry {
                original: entity.text.clone(),
                created_at: Instant::now(),
            },
        );
        token
    }

    /// 还原文本中所有 token 为原始 PII（过期 token 不还原）
    pub fn detokenize(&self, text: &str) -> String {
        let mut result = text.to_string();
        let now = Instant::now();
        for (token, entry) in &self.reverse {
            if self.is_expired(now, entry.created_at) {
                continue;
            }
            result = result.replace(token.as_str(), &entry.original);
        }
        result
    }

    /// 清理过期条目，返回已销毁 token 列表和销毁证明
    pub fn cleanup_expired(&mut self) -> Option<(Vec<String>, DestructionProof)> {
        let ttl = self.ttl?;
        let now = Instant::now();
        let expired_tokens: Vec<String> = self
            .reverse
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.created_at) >= ttl)
            .map(|(token, _)| token.clone())
            .collect();

        if expired_tokens.is_empty() {
            return None;
        }

        let mut destroyed_originals = Vec::new();
        for token in &expired_tokens {
            if let Some(mut entry) = self.reverse.remove(token) {
                // 从 forward 中也移除
                self.forward.retain(|_, v| v != token);
                destroyed_originals.push(entry.original.clone());
                entry.original.zeroize();
            }
        }

        let proof = DestructionProof::generate(
            &self.signing_key,
            &destroyed_originals,
            "ttl_expire_zeroize",
        );

        Some((expired_tokens, proof))
    }

    /// 安全擦除所有映射和密钥，生成销毁证明
    pub fn destroy_with_proof(&mut self) -> DestructionProof {
        let originals: Vec<String> = self.reverse.values().map(|e| e.original.clone()).collect();
        let proof = DestructionProof::generate(&self.signing_key, &originals, "zeroize");
        self.destroy();
        proof
    }

    /// 安全擦除所有映射和密钥
    pub fn destroy(&mut self) {
        for (mut k, _) in self.forward.drain() {
            k.zeroize();
        }
        for (mut k, mut v) in self.reverse.drain() {
            k.zeroize();
            v.original.zeroize();
        }
        self.cipher_key.zeroize();
        self.signing_key.zeroize();
        self.counters.clear();
    }

    fn is_expired(&self, now: Instant, created_at: Instant) -> bool {
        match self.ttl {
            Some(ttl) => now.duration_since(created_at) >= ttl,
            None => false,
        }
    }

    /// 导出加密快照（AES-256-GCM）
    pub fn export_encrypted(&self) -> Vec<u8> {
        use aes_gcm::aead::Aead;
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
        let cipher = Aes256Gcm::new_from_slice(&self.cipher_key).expect("invalid key");
        let mut nonce_bytes = [0u8; 12];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut nonce_bytes)
            .expect("failed to generate nonce");
        let nonce = Nonce::from_slice(&nonce_bytes);
        let data = serde_json::to_vec(&self.forward).unwrap_or_default();
        let encrypted = cipher
            .encrypt(nonce, data.as_ref())
            .expect("encryption failed");
        let mut output = nonce_bytes.to_vec();
        output.extend(encrypted);
        output
    }

    /// 统计信息
    pub fn stats(&self) -> VaultStats {
        let now = Instant::now();
        let expired_count = self
            .reverse
            .values()
            .filter(|e| self.is_expired(now, e.created_at))
            .count();
        VaultStats {
            total_tokens: self.forward.len(),
            by_type: self.counters.clone(),
            expired_count,
        }
    }
}

impl Default for TokenVault {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TokenVault {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::pii_detector::{PiiEntity, PiiType};

    fn make_entity(text: &str, pii_type: PiiType) -> PiiEntity {
        PiiEntity {
            entity_type: pii_type,
            start: 0,
            end: text.len(),
            text: text.to_string(),
            confidence: 0.95,
        }
    }

    #[test]
    fn roundtrip_tokenize_detokenize() {
        let mut vault = TokenVault::new();
        let entity = make_entity("张三", PiiType::ChineseName);
        let token = vault.tokenize(&entity);
        assert!(token.starts_with("[PERSON_"));
        let restored = vault.detokenize(&format!("用户{}你好", token));
        assert_eq!(restored, "用户张三你好");
    }

    #[test]
    fn same_pii_same_token() {
        let mut vault = TokenVault::new();
        let e1 = make_entity("test@mail.com", PiiType::Email);
        let e2 = make_entity("test@mail.com", PiiType::Email);
        let t1 = vault.tokenize(&e1);
        let t2 = vault.tokenize(&e2);
        assert_eq!(t1, t2);
    }

    #[test]
    fn destroy_clears_all() {
        let mut vault = TokenVault::new();
        let entity = make_entity("13812345678", PiiType::ChinesePhone);
        let token = vault.tokenize(&entity);
        vault.destroy();
        let result = vault.detokenize(&token);
        assert_eq!(result, token); // 无法还原
    }

    #[test]
    fn different_types_different_prefix() {
        let mut vault = TokenVault::new();
        let t1 = vault.tokenize(&make_entity("a@b.com", PiiType::Email));
        let t2 = vault.tokenize(&make_entity("13800000000", PiiType::ChinesePhone));
        assert!(t1.starts_with("[EMAIL_"));
        assert!(t2.starts_with("[PHONE_"));
    }

    #[test]
    fn stats_correct() {
        let mut vault = TokenVault::new();
        vault.tokenize(&make_entity("a@b.com", PiiType::Email));
        vault.tokenize(&make_entity("c@d.com", PiiType::Email));
        vault.tokenize(&make_entity("13800000000", PiiType::ChinesePhone));
        let stats = vault.stats();
        assert_eq!(stats.total_tokens, 3);
    }

    #[test]
    fn ttl_expired_token_not_restored() {
        // TTL = 0ms → 立即过期
        let mut vault = TokenVault::new_with_ttl(Duration::from_millis(0));
        let entity = make_entity("secret@mail.com", PiiType::Email);
        let token = vault.tokenize(&entity);
        // token 已过期，detokenize 不应还原
        let result = vault.detokenize(&token);
        assert_eq!(result, token);
    }

    #[test]
    fn no_ttl_never_expires() {
        let mut vault = TokenVault::new(); // 无 TTL
        let entity = make_entity("test@mail.com", PiiType::Email);
        let token = vault.tokenize(&entity);
        let result = vault.detokenize(&token);
        assert_eq!(result, "test@mail.com");
    }

    #[test]
    fn cleanup_expired_generates_proof() {
        let mut vault = TokenVault::new_with_ttl(Duration::from_millis(0));
        vault.tokenize(&make_entity("a@b.com", PiiType::Email));
        vault.tokenize(&make_entity("13800000000", PiiType::ChinesePhone));

        let result = vault.cleanup_expired();
        assert!(result.is_some());
        let (tokens, proof) = result.unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(proof.items_destroyed, 2);
        assert_eq!(proof.method, "ttl_expire_zeroize");
        // vault 应该为空
        assert_eq!(vault.stats().total_tokens, 0);
    }

    #[test]
    fn destroy_with_proof_generates_valid_proof() {
        let mut vault = TokenVault::new();
        vault.tokenize(&make_entity("data1", PiiType::Email));
        vault.tokenize(&make_entity("data2", PiiType::ChinesePhone));
        let proof = vault.destroy_with_proof();
        assert_eq!(proof.items_destroyed, 2);
        assert_eq!(proof.method, "zeroize");
    }

    #[test]
    fn cleanup_no_ttl_returns_none() {
        let mut vault = TokenVault::new(); // 无 TTL
        vault.tokenize(&make_entity("a@b.com", PiiType::Email));
        assert!(vault.cleanup_expired().is_none());
    }
}
