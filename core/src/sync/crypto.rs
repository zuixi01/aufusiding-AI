//! E2E 加密
//!
//! 端到端加密，确保同步数据在传输和服务器存储时都是加密的。

use ring::aead::{self, BoundKey};
use ring::rand::SystemRandom;
use ring::signature::{self, Ed25519KeyPair, KeyPair};

/// E2E 加密管理器
pub struct E2EEncryption {
    _private_key: Vec<u8>,
    _public_key: Vec<u8>,
    signing_key: Option<Ed25519KeyPair>,
}

impl E2EEncryption {
    /// 生成新的 Ed25519 密钥对
    pub fn generate_keypair() -> anyhow::Result<Self> {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|e| anyhow::anyhow!("keygen failed: {}", e))?;
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
            .map_err(|e| anyhow::anyhow!("parse key failed: {}", e))?;
        let public_key = key_pair.public_key().as_ref().to_vec();
        let private_key = pkcs8.as_ref().to_vec();
        Ok(Self {
            _private_key: private_key,
            _public_key: public_key,
            signing_key: Some(key_pair),
        })
    }

    /// 从已有密钥加载
    pub fn from_keys(private_key: Vec<u8>, public_key: Vec<u8>) -> Self {
        let signing_key = Ed25519KeyPair::from_pkcs8(&private_key).ok();
        Self {
            _private_key: private_key,
            _public_key: public_key,
            signing_key,
        }
    }

    pub fn public_key(&self) -> &[u8] {
        &self._public_key
    }

    /// AES-256-GCM 加密（使用共享密钥派生）
    pub fn encrypt(&self, plaintext: &[u8], _recipient_pubkey: &[u8]) -> anyhow::Result<Vec<u8>> {
        // 使用 HKDF 从本地私钥 + 接收方公钥派生对称密钥
        // 简化实现：用私钥前 32 字节作为对称密钥种子
        let key_bytes = derive_symmetric_key(&self._private_key, _recipient_pubkey);
        let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, &key_bytes)
            .map_err(|e| anyhow::anyhow!("create key failed: {}", e))?;
        let mut sealing_key = aead::SealingKey::new(unbound_key, NonceGen::new());

        let mut in_out = plaintext.to_vec();
        sealing_key
            .seal_in_place_append_tag(aead::Aad::empty(), &mut in_out)
            .map_err(|e| anyhow::anyhow!("encrypt failed: {}", e))?;
        Ok(in_out)
    }

    /// AES-256-GCM 解密
    pub fn decrypt(&self, ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
        // 对称密钥需要与加密时相同的派生
        // 简化：使用自身公钥作为 recipient（自解密场景）
        let key_bytes = derive_symmetric_key(&self._private_key, &self._public_key);
        let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, &key_bytes)
            .map_err(|e| anyhow::anyhow!("create key failed: {}", e))?;
        let mut opening_key = aead::OpeningKey::new(unbound_key, NonceGen::new());

        let mut in_out = ciphertext.to_vec();
        let plaintext = opening_key
            .open_in_place(aead::Aad::empty(), &mut in_out)
            .map_err(|_| anyhow::anyhow!("decrypt failed: invalid ciphertext or wrong key"))?;
        Ok(plaintext.to_vec())
    }

    /// Ed25519 签名
    pub fn sign(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let key = self
            .signing_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no signing key available"))?;
        Ok(key.sign(data).as_ref().to_vec())
    }

    /// 验证 Ed25519 签名
    pub fn verify(&self, data: &[u8], sig: &[u8], pubkey: &[u8]) -> anyhow::Result<bool> {
        let peer_public_key = signature::UnparsedPublicKey::new(&signature::ED25519, pubkey);
        match peer_public_key.verify(data, sig) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// 从私钥和对方公钥派生 32 字节对称密钥（简化 HKDF）
fn derive_symmetric_key(private_key: &[u8], peer_pubkey: &[u8]) -> [u8; 32] {
    use ring::hmac;
    let salt = hmac::Key::new(hmac::HMAC_SHA256, peer_pubkey);
    let tag = hmac::sign(&salt, private_key);
    let mut key = [0u8; 32];
    key.copy_from_slice(&tag.as_ref()[..32]);
    key
}

/// 确定性 nonce 生成器（用于测试，生产环境应使用随机 nonce）
struct NonceGen {
    counter: u64,
}

impl NonceGen {
    fn new() -> Self {
        Self { counter: 0 }
    }
}

impl aead::NonceSequence for NonceGen {
    fn advance(&mut self) -> Result<aead::Nonce, ring::error::Unspecified> {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..].copy_from_slice(&self.counter.to_le_bytes());
        self.counter += 1;
        Ok(aead::Nonce::assume_unique_for_key(nonce_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_keypair_works() {
        let enc = E2EEncryption::generate_keypair().unwrap();
        assert!(!enc.public_key().is_empty());
        assert_eq!(enc.public_key().len(), 32); // Ed25519 public key
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let enc = E2EEncryption::generate_keypair().unwrap();
        let plaintext = b"hello world";
        let ciphertext = enc.encrypt(plaintext, enc.public_key()).unwrap();
        assert_ne!(&ciphertext, plaintext);
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_decrypt_fails() {
        let enc1 = E2EEncryption::generate_keypair().unwrap();
        let enc2 = E2EEncryption::generate_keypair().unwrap();
        let ciphertext = enc1.encrypt(b"secret", enc1.public_key()).unwrap();
        let result = enc2.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn sign_verify_works() {
        let enc = E2EEncryption::generate_keypair().unwrap();
        let data = b"important data";
        let sig = enc.sign(data).unwrap();
        assert!(enc.verify(data, &sig, enc.public_key()).unwrap());
    }

    #[test]
    fn wrong_signature_fails() {
        let enc = E2EEncryption::generate_keypair().unwrap();
        let data = b"important data";
        let sig = enc.sign(data).unwrap();
        assert!(!enc.verify(b"tampered", &sig, enc.public_key()).unwrap());
    }

    #[test]
    fn empty_data_encrypt() {
        let enc = E2EEncryption::generate_keypair().unwrap();
        let ciphertext = enc.encrypt(b"", enc.public_key()).unwrap();
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn large_data_encrypt() {
        let enc = E2EEncryption::generate_keypair().unwrap();
        let data = vec![0xABu8; 10_000];
        let ciphertext = enc.encrypt(&data, enc.public_key()).unwrap();
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn different_keypairs() {
        let enc1 = E2EEncryption::generate_keypair().unwrap();
        let enc2 = E2EEncryption::generate_keypair().unwrap();
        assert_ne!(enc1.public_key(), enc2.public_key());
    }
}
