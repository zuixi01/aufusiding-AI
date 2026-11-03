//! 脱敏配置
//!
//! 控制 PII 检测的实体类型、置信度阈值和安全策略。

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use super::pii_detector::PiiType;
use super::sanitization_strategy::StrategyKind;

/// 脱敏配置
#[derive(Debug, Clone)]
pub struct SanitizerConfig {
    /// 是否启用脱敏
    pub enabled: bool,
    /// 启用的实体类型（空集 = 全部启用）
    pub entity_types: HashSet<PiiType>,
    /// 最低置信度阈值
    pub min_confidence: f32,
    /// 是否记录检测日志（脱敏后的）
    pub log_detections: bool,
    /// 高风险数据是否直接阻止发送
    pub block_on_high_risk: bool,
    /// Token 映射存活时间
    pub vault_ttl: Duration,
    /// 启用 DataClassifier 集成（分级策略引擎）
    pub classification_enabled: bool,
    /// 默认脱敏策略（未启用分级时使用）
    pub default_strategy: StrategyKind,
    /// 启用审计日志
    pub audit_enabled: bool,
    /// 密钥轮换间隔
    pub key_rotation_interval: Duration,
    /// L3 机密数据强制不可逆
    pub irreversible_for_confidential: bool,
    /// 外部词典路径
    pub dictionary_paths: Vec<PathBuf>,
    /// 启用上下文泄露检测
    pub context_leak_detection: bool,
    /// 自定义敏感环境变量前缀
    pub custom_env_prefixes: Vec<String>,
}

impl Default for SanitizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            entity_types: HashSet::new(),
            min_confidence: 0.8,
            log_detections: false,
            block_on_high_risk: false,
            vault_ttl: Duration::from_secs(3600),
            classification_enabled: false,
            default_strategy: StrategyKind::Tokenize,
            audit_enabled: false,
            key_rotation_interval: Duration::from_secs(86400),
            irreversible_for_confidential: false,
            dictionary_paths: Vec::new(),
            context_leak_detection: false,
            custom_env_prefixes: Vec::new(),
        }
    }
}

impl SanitizerConfig {
    /// 检查某实体类型是否启用
    pub fn is_type_enabled(&self, pii_type: &PiiType) -> bool {
        self.entity_types.is_empty() || self.entity_types.contains(pii_type)
    }

    /// 构建器：仅启用指定类型
    pub fn with_types(mut self, types: Vec<PiiType>) -> Self {
        self.entity_types = types.into_iter().collect();
        self
    }

    /// 构建器：设置置信度阈值
    pub fn with_min_confidence(mut self, confidence: f32) -> Self {
        self.min_confidence = confidence;
        self
    }

    /// 军工级配置：一键启用所有强化特性
    pub fn military_grade() -> Self {
        Self {
            enabled: true,
            entity_types: HashSet::new(),
            min_confidence: 0.7,
            log_detections: true,
            block_on_high_risk: true,
            vault_ttl: Duration::from_secs(1800),
            classification_enabled: true,
            default_strategy: StrategyKind::Redact,
            audit_enabled: true,
            key_rotation_interval: Duration::from_secs(3600),
            irreversible_for_confidential: true,
            dictionary_paths: Vec::new(),
            context_leak_detection: true,
            custom_env_prefixes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_enables_all_types() {
        let config = SanitizerConfig::default();
        assert!(config.is_type_enabled(&PiiType::Email));
        assert!(config.is_type_enabled(&PiiType::ChinesePhone));
        assert!(config.is_type_enabled(&PiiType::ChineseIdCard));
    }

    #[test]
    fn with_types_restricts() {
        let config =
            SanitizerConfig::default().with_types(vec![PiiType::Email, PiiType::ChinesePhone]);
        assert!(config.is_type_enabled(&PiiType::Email));
        assert!(!config.is_type_enabled(&PiiType::IpAddress));
    }

    #[test]
    fn min_confidence_builder() {
        let config = SanitizerConfig::default().with_min_confidence(0.5);
        assert!((config.min_confidence - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn default_has_new_fields_off() {
        let config = SanitizerConfig::default();
        assert!(!config.classification_enabled);
        assert!(!config.audit_enabled);
        assert!(!config.irreversible_for_confidential);
        assert_eq!(config.default_strategy, StrategyKind::Tokenize);
    }

    #[test]
    fn military_grade_enables_all() {
        let config = SanitizerConfig::military_grade();
        assert!(config.classification_enabled);
        assert!(config.audit_enabled);
        assert!(config.irreversible_for_confidential);
        assert!(config.block_on_high_risk);
        assert_eq!(config.default_strategy, StrategyKind::Redact);
    }
}
