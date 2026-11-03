//! 数据销毁证明
//!
//! 为 TokenVault 销毁操作生成可独立验证的密码学证明。

use chrono::{DateTime, Utc};
use ring::digest;
use ring::hmac;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 数据销毁证明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestructionProof {
    /// 证明唯一 ID
    pub proof_id: String,
    /// 销毁时间戳
    pub timestamp: DateTime<Utc>,
    /// 被销毁数据的 SHA-256 哈希列表
    pub data_hashes: Vec<String>,
    /// 销毁方法
    pub method: String,
    /// 销毁条目数
    pub items_destroyed: usize,
    /// HMAC-SHA256 签名（hex 编码）
    pub signature: String,
}

impl DestructionProof {
    /// 为一组被销毁的原始数据值生成销毁证明
    ///
    /// `signing_key` 用于 HMAC 签名，`originals` 是被销毁的原文列表。
    pub fn generate(signing_key: &[u8], originals: &[String], method: &str) -> Self {
        let proof_id = Uuid::new_v4().to_string();
        let timestamp = Utc::now();
        let data_hashes: Vec<String> = originals
            .iter()
            .map(|s| {
                let d = digest::digest(&digest::SHA256, s.as_bytes());
                hex_encode(d.as_ref())
            })
            .collect();
        let items_destroyed = originals.len();

        // 构造待签名载荷：proof_id || timestamp || method || items_destroyed || all hashes
        let payload =
            Self::build_payload(&proof_id, &timestamp, method, items_destroyed, &data_hashes);
        let key = hmac::Key::new(hmac::HMAC_SHA256, signing_key);
        let tag = hmac::sign(&key, payload.as_bytes());
        let signature = hex_encode(tag.as_ref());

        Self {
            proof_id,
            timestamp,
            data_hashes,
            method: method.to_string(),
            items_destroyed,
            signature,
        }
    }

    /// 验证销毁证明的签名完整性
    pub fn verify(&self, signing_key: &[u8]) -> bool {
        let payload = Self::build_payload(
            &self.proof_id,
            &self.timestamp,
            &self.method,
            self.items_destroyed,
            &self.data_hashes,
        );
        let key = hmac::Key::new(hmac::HMAC_SHA256, signing_key);
        let expected = hmac::sign(&key, payload.as_bytes());
        hex_encode(expected.as_ref()) == self.signature
    }

    fn build_payload(
        proof_id: &str,
        timestamp: &DateTime<Utc>,
        method: &str,
        items_destroyed: usize,
        data_hashes: &[String],
    ) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            proof_id,
            timestamp.to_rfc3339(),
            method,
            items_destroyed,
            data_hashes.join(","),
        )
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_verify() {
        let key = b"test-signing-key-for-destruction";
        let originals = vec!["secret@mail.com".to_string(), "13812345678".to_string()];
        let proof = DestructionProof::generate(key, &originals, "zeroize");

        assert_eq!(proof.items_destroyed, 2);
        assert_eq!(proof.data_hashes.len(), 2);
        assert_eq!(proof.method, "zeroize");
        assert!(proof.verify(key));
    }

    #[test]
    fn tampered_proof_fails_verification() {
        let key = b"test-signing-key-for-destruction";
        let originals = vec!["data1".to_string()];
        let mut proof = DestructionProof::generate(key, &originals, "zeroize");
        proof.items_destroyed = 999;
        assert!(!proof.verify(key));
    }

    #[test]
    fn wrong_key_fails_verification() {
        let key = b"correct-key-xxxxxxxxxxxxxxxxxx";
        let originals = vec!["data".to_string()];
        let proof = DestructionProof::generate(key, &originals, "zeroize");
        assert!(!proof.verify(b"wrong-key-xxxxxxxxxxxxxxxxxxxxxx"));
    }

    #[test]
    fn data_hashes_are_sha256() {
        let key = b"test-key-for-hash-verification!";
        let originals = vec!["hello".to_string()];
        let proof = DestructionProof::generate(key, &originals, "zeroize");
        // SHA-256 of "hello" is well-known
        let expected = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert_eq!(proof.data_hashes[0], expected);
    }
}
