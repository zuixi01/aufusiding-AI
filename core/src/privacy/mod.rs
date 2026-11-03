//! GDPR 合规模块
//!
//! 提供同意管理、数据导出、数据删除、数据匿名化等隐私保护功能。
//! 新增：可逆数据脱敏层（SanitizingProvider + TokenVault）。
//! 军工级强化：多策略引擎、分级策略、防篡改审计、密钥管理、数据销毁证明。
//! 全谱检测：领域词典（SensitiveDictionary）+ 上下文泄露（ContextLeakDetector）。

pub mod anonymization;
pub mod audit_log;
pub mod consent;
pub mod context_leak_detector;
pub mod deletion;
pub mod destruction_proof;
pub mod export;
pub mod key_manager;
pub mod pii_detector;
pub mod sanitization_policy;
pub mod sanitization_strategy;
pub mod sanitizer_config;
pub mod sanitizing_provider;
pub mod sensitive_dictionary;
#[cfg(test)]
mod tests_sanitizer;
pub mod token_vault;

pub use anonymization::Anonymizer;
pub use audit_log::{AuditOperation, SanitizationAuditLog};
pub use consent::{ConsentManager, ConsentPurpose, ConsentRecord};
pub use context_leak_detector::ContextLeakDetector;
pub use deletion::{DataEraser, DeletionReceipt};
pub use destruction_proof::DestructionProof;
pub use export::DataExporter;
pub use key_manager::{KeyManager, KeyPurpose};
pub use pii_detector::{PiiDetector, PiiEntity, PiiType};
pub use sanitization_policy::SanitizationPolicy;
pub use sanitization_strategy::{MockSanitizationStrategy, SanitizationStrategy, StrategyKind};
pub use sanitizer_config::SanitizerConfig;
pub use sanitizing_provider::SanitizingProvider;
pub use sensitive_dictionary::SensitiveDictionary;
pub use token_vault::TokenVault;
