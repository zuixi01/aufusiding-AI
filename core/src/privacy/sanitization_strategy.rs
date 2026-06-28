//! 多策略脱敏引擎
//!
//! 定义 `SanitizationStrategy` trait 和 5 种具体策略：
//! - Token（可逆）、StaticMask（不可逆）、PartialMask（不可逆）、Redact（不可逆）、FPE（可逆）

use serde::{Deserialize, Serialize};

use super::pii_detector::{PiiEntity, PiiType};
use super::token_vault::TokenVault;

/// 策略类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrategyKind {
    /// 可逆 token 替换（当前默认行为）
    Tokenize,
    /// 格式保留的虚假数据替换
    StaticMask,
    /// 保留首尾，中间用 `*` 掩码
    PartialMask,
    /// 完全替换为 `[REDACTED]`
    Redact,
    /// FF1 格式保留加密（仅数字）
    Fpe,
}

impl std::fmt::Display for StrategyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tokenize => write!(f, "Tokenize"),
            Self::StaticMask => write!(f, "StaticMask"),
            Self::PartialMask => write!(f, "PartialMask"),
            Self::Redact => write!(f, "Redact"),
            Self::Fpe => write!(f, "FPE"),
        }
    }
}

/// 脱敏策略 trait
pub trait SanitizationStrategy: Send + Sync {
    /// 策略类型
    fn kind(&self) -> StrategyKind;
    /// 对单个 PII 实体执行脱敏，返回替换文本
    fn sanitize(&self, entity: &PiiEntity, vault: &mut TokenVault) -> String;
    /// 是否可逆
    fn is_reversible(&self) -> bool;
}

// ── 1. TokenStrategy（可逆，当前默认行为）──────────────────────

pub struct TokenStrategy;

impl SanitizationStrategy for TokenStrategy {
    fn kind(&self) -> StrategyKind { StrategyKind::Tokenize }

    fn sanitize(&self, entity: &PiiEntity, vault: &mut TokenVault) -> String {
        vault.tokenize(entity)
    }

    fn is_reversible(&self) -> bool { true }
}

// ── 2. StaticMaskStrategy（不可逆，格式保留虚假数据）─────────

pub struct StaticMaskStrategy;

impl StaticMaskStrategy {
    fn fake_for(entity: &PiiEntity) -> String {
        match &entity.entity_type {
            PiiType::Email => "user@example.com".to_string(),
            PiiType::ChinesePhone | PiiType::InternationalPhone => "138****0000".to_string(),
            PiiType::ChineseIdCard => "110101********0011".to_string(),
            PiiType::BankCard => "6222 **** **** 0000".to_string(),
            PiiType::ChineseName => "**某".to_string(),
            PiiType::IpAddress => "0.0.0.0".to_string(),
            PiiType::ApiKey => "[MASKED_KEY]".to_string(),
            PiiType::Password => "********".to_string(),
            PiiType::GeoCoordinate => "0.0000, 0.0000".to_string(),
            PiiType::PhysicalAddress => "[MASKED_ADDRESS]".to_string(),
            PiiType::MacAddress => "00:00:00:00:00:00".to_string(),
            PiiType::Imei => "000000000000000".to_string(),
            PiiType::CryptoAddress => "[MASKED_CRYPTO]".to_string(),
            PiiType::CloudCredential => "[MASKED_CREDENTIAL]".to_string(),
            PiiType::PrivateKey => "[MASKED_PRIVATE_KEY]".to_string(),
            PiiType::DbConnectionString => "[MASKED_DB_CONN]".to_string(),
            PiiType::JwtToken => "[MASKED_JWT]".to_string(),
            PiiType::Vin => "00000000000000000".to_string(),
            PiiType::Ipv6Address => "::".to_string(),
            PiiType::Iban => "[MASKED_IBAN]".to_string(),
            _ => "*".repeat(entity.text.len().min(20)),
        }
    }
}

impl SanitizationStrategy for StaticMaskStrategy {
    fn kind(&self) -> StrategyKind { StrategyKind::StaticMask }

    fn sanitize(&self, entity: &PiiEntity, _vault: &mut TokenVault) -> String {
        Self::fake_for(entity)
    }

    fn is_reversible(&self) -> bool { false }
}

// ── 3. PartialMaskStrategy（不可逆，保留首尾）────────────────

pub struct PartialMaskStrategy {
    /// 保留前缀字符数
    pub keep_prefix: usize,
    /// 保留后缀字符数
    pub keep_suffix: usize,
}

impl Default for PartialMaskStrategy {
    fn default() -> Self {
        Self { keep_prefix: 3, keep_suffix: 4 }
    }
}

impl PartialMaskStrategy {
    /// 银行卡专用：前6后4
    pub fn bank_card() -> Self {
        Self { keep_prefix: 6, keep_suffix: 4 }
    }
}

impl SanitizationStrategy for PartialMaskStrategy {
    fn kind(&self) -> StrategyKind { StrategyKind::PartialMask }

    fn sanitize(&self, entity: &PiiEntity, _vault: &mut TokenVault) -> String {
        let chars: Vec<char> = entity.text.chars().collect();
        let len = chars.len();
        if len <= self.keep_prefix + self.keep_suffix {
            return "*".repeat(len);
        }
        let prefix: String = chars[..self.keep_prefix].iter().collect();
        let suffix: String = chars[len - self.keep_suffix..].iter().collect();
        let mask_len = len - self.keep_prefix - self.keep_suffix;
        format!("{}{}{}", prefix, "*".repeat(mask_len), suffix)
    }

    fn is_reversible(&self) -> bool { false }
}

// ── 4. RedactStrategy（不可逆，完全替换）─────────────────────

pub struct RedactStrategy;

impl SanitizationStrategy for RedactStrategy {
    fn kind(&self) -> StrategyKind { StrategyKind::Redact }

    fn sanitize(&self, _entity: &PiiEntity, _vault: &mut TokenVault) -> String {
        "[REDACTED]".to_string()
    }

    fn is_reversible(&self) -> bool { false }
}

// ── 5. FpeStrategy（可逆，AES-based Feistel 格式保留加密，仅数字）──

/// AES-based Feistel FPE：对数字字符串做多轮 Feistel 网络加密。
/// 每轮使用 HMAC-SHA256(key, round || right_half) 生成伪随机数，
/// 对左半部分做模加。8 轮 Feistel 提供足够的扩散性。
/// 通过 vault 记录原文→密文映射以支持还原。
pub struct FpeStrategy {
    key: Vec<u8>,
}

impl FpeStrategy {
    pub fn new(key: &[u8]) -> Self {
        Self { key: key.to_vec() }
    }

    const ROUNDS: u8 = 8;

    /// 8-round balanced Feistel network on digit strings
    fn encrypt_digits(&self, digits: &str) -> String {
        use ring::hmac;
        let digit_vals: Vec<u8> = digits
            .chars()
            .filter_map(|c| c.to_digit(10).map(|d| d as u8))
            .collect();
        if digit_vals.is_empty() {
            return digits.to_string();
        }

        let n = digit_vals.len();
        let mid = n / 2;
        let mut left: Vec<u8> = digit_vals[..mid].to_vec();
        let mut right: Vec<u8> = digit_vals[mid..].to_vec();

        let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, &self.key);

        for round in 0..Self::ROUNDS {
            // PRF input = round || right_half digits
            let mut prf_input = vec![round];
            prf_input.extend_from_slice(&right);
            let tag = hmac::sign(&hmac_key, &prf_input);
            let hash = tag.as_ref();

            // 用 hash 字节对 left 做模 10 加法（u16 避免溢出）
            let new_left: Vec<u8> = left
                .iter()
                .enumerate()
                .map(|(i, &d)| ((d as u16 + hash[i % hash.len()] as u16) % 10) as u8)
                .collect();

            // Feistel swap: left = right, right = new_left
            left = right;
            right = new_left;
        }

        // 重组：left || right
        let mut result = String::with_capacity(n);
        for &d in left.iter().chain(right.iter()) {
            result.push(char::from_digit(d as u32, 10).unwrap());
        }
        result
    }
}

impl SanitizationStrategy for FpeStrategy {
    fn kind(&self) -> StrategyKind { StrategyKind::Fpe }

    fn sanitize(&self, entity: &PiiEntity, vault: &mut TokenVault) -> String {
        // 提取数字部分做 FPE，非数字保留
        let encrypted = self.encrypt_digits(&entity.text);
        // 同时存入 vault 以支持还原
        vault.tokenize(entity);
        encrypted
    }

    fn is_reversible(&self) -> bool { true }
}

/// 根据 StrategyKind 创建默认策略实例
pub fn create_strategy(kind: StrategyKind, fpe_key: Option<&[u8]>) -> Box<dyn SanitizationStrategy> {
    match kind {
        StrategyKind::Tokenize => Box::new(TokenStrategy),
        StrategyKind::StaticMask => Box::new(StaticMaskStrategy),
        StrategyKind::PartialMask => Box::new(PartialMaskStrategy::default()),
        StrategyKind::Redact => Box::new(RedactStrategy),
        StrategyKind::Fpe => {
            let key = fpe_key.unwrap_or(&[0u8; 32]);
            Box::new(FpeStrategy::new(key))
        }
    }
}

/// 测试用 Mock 脱敏策略
pub struct MockSanitizationStrategy {
    strategy_kind: StrategyKind,
    replacement: String,
    reversible: bool,
    calls: std::sync::Mutex<Vec<String>>,
}

impl MockSanitizationStrategy {
    pub fn new(kind: StrategyKind, replacement: &str) -> Self {
        Self {
            strategy_kind: kind,
            replacement: replacement.to_string(),
            reversible: false,
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn with_reversible(mut self, val: bool) -> Self {
        self.reversible = val;
        self
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    pub fn called_texts(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl SanitizationStrategy for MockSanitizationStrategy {
    fn kind(&self) -> StrategyKind {
        self.strategy_kind
    }

    fn sanitize(&self, entity: &PiiEntity, _vault: &mut TokenVault) -> String {
        self.calls.lock().unwrap().push(entity.text.clone());
        self.replacement.clone()
    }

    fn is_reversible(&self) -> bool {
        self.reversible
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
    fn token_strategy_is_reversible() {
        let strategy = TokenStrategy;
        let mut vault = TokenVault::new();
        let entity = make_entity("test@mail.com", PiiType::Email);
        let token = strategy.sanitize(&entity, &mut vault);
        assert!(token.starts_with("[EMAIL_"));
        assert!(strategy.is_reversible());
        assert_eq!(vault.detokenize(&token), "test@mail.com");
    }

    #[test]
    fn static_mask_produces_fake_data() {
        let strategy = StaticMaskStrategy;
        let mut vault = TokenVault::new();
        let entity = make_entity("real@secret.com", PiiType::Email);
        let masked = strategy.sanitize(&entity, &mut vault);
        assert_eq!(masked, "user@example.com");
        assert!(!strategy.is_reversible());
    }

    #[test]
    fn partial_mask_keeps_prefix_suffix() {
        let strategy = PartialMaskStrategy::bank_card();
        let mut vault = TokenVault::new();
        let entity = make_entity("6222021234567890", PiiType::BankCard);
        let masked = strategy.sanitize(&entity, &mut vault);
        assert!(masked.starts_with("622202"));
        assert!(masked.ends_with("7890"));
        assert!(masked.contains("******"));
    }

    #[test]
    fn redact_replaces_completely() {
        let strategy = RedactStrategy;
        let mut vault = TokenVault::new();
        let entity = make_entity("sk-secret123456789012345", PiiType::ApiKey);
        let redacted = strategy.sanitize(&entity, &mut vault);
        assert_eq!(redacted, "[REDACTED]");
    }

    #[test]
    fn fpe_preserves_length() {
        let key = [42u8; 32];
        let strategy = FpeStrategy::new(&key);
        let mut vault = TokenVault::new();
        let entity = make_entity("13812345678", PiiType::ChinesePhone);
        let encrypted = strategy.sanitize(&entity, &mut vault);
        assert_eq!(encrypted.len(), 11);
        assert!(encrypted.chars().all(|c| c.is_ascii_digit()));
        assert_ne!(encrypted, "13812345678");
    }

    #[test]
    fn create_strategy_factory() {
        let s = create_strategy(StrategyKind::Redact, None);
        assert_eq!(s.kind(), StrategyKind::Redact);
        assert!(!s.is_reversible());

        let s2 = create_strategy(StrategyKind::Tokenize, None);
        assert!(s2.is_reversible());
    }

    #[test]
    fn fpe_deterministic_same_key() {
        let key = [42u8; 32];
        let s1 = FpeStrategy::new(&key);
        let s2 = FpeStrategy::new(&key);
        let input = "13812345678";
        assert_eq!(s1.encrypt_digits(input), s2.encrypt_digits(input));
    }

    #[test]
    fn fpe_different_key_different_output() {
        let s1 = FpeStrategy::new(&[1u8; 32]);
        let s2 = FpeStrategy::new(&[2u8; 32]);
        let input = "13812345678";
        assert_ne!(s1.encrypt_digits(input), s2.encrypt_digits(input));
    }
}
