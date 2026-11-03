//! 分级策略引擎
//!
//! 桥接 DataClassifier（L0-L3）与 PiiDetector，按敏感度自动选择脱敏策略。

use std::collections::HashSet;

use super::pii_detector::{PiiEntity, PiiType};
use super::sanitization_strategy::StrategyKind;
use crate::safety::classification::{DataClassification, DataClassifier, SensitivityLevel};

/// 单个实体的策略决议
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub entity: PiiEntity,
    pub strategy: StrategyKind,
}

/// 分级策略引擎
pub struct SanitizationPolicy {
    /// 是否启用分级（关闭时全部使用 Tokenize）
    enabled: bool,
}

impl SanitizationPolicy {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// 根据分级结果和 PII 实体列表，为每个实体分配脱敏策略
    pub fn resolve(
        &self,
        classification: &DataClassification,
        entities: &[PiiEntity],
    ) -> Vec<PolicyDecision> {
        if !self.enabled || entities.is_empty() {
            return entities
                .iter()
                .map(|e| PolicyDecision {
                    entity: e.clone(),
                    strategy: StrategyKind::Tokenize,
                })
                .collect();
        }

        // 组合升级：≥3 种不同 PII 类型时，敏感度自动提升一级
        let effective_level = self.maybe_escalate(classification, entities);

        entities
            .iter()
            .map(|e| {
                let strategy = self.select_strategy(effective_level, e);
                PolicyDecision {
                    entity: e.clone(),
                    strategy,
                }
            })
            .collect()
    }

    /// 组合升级：≥3 种不同 PII 类型时提升一级
    fn maybe_escalate(
        &self,
        classification: &DataClassification,
        entities: &[PiiEntity],
    ) -> SensitivityLevel {
        let distinct_types: HashSet<String> =
            entities.iter().map(|e| e.entity_type.to_string()).collect();

        let base = classification.level;
        if distinct_types.len() >= 3 {
            match base {
                SensitivityLevel::Public => SensitivityLevel::Normal,
                SensitivityLevel::Normal => SensitivityLevel::Sensitive,
                SensitivityLevel::Sensitive => SensitivityLevel::Confidential,
                SensitivityLevel::Confidential => SensitivityLevel::Confidential,
            }
        } else {
            base
        }
    }

    /// 按敏感度 + PII 类型选择策略
    fn select_strategy(&self, level: SensitivityLevel, entity: &PiiEntity) -> StrategyKind {
        match level {
            // L3 机密 + 任何 PII → Redact（不可逆，零容忍）
            SensitivityLevel::Confidential => StrategyKind::Redact,

            // L2 敏感：按 PII 类型细分
            SensitivityLevel::Sensitive => match &entity.entity_type {
                PiiType::Password | PiiType::ApiKey => StrategyKind::Redact,
                // 高危凭证类 → Redact
                PiiType::PrivateKey | PiiType::CloudCredential | PiiType::DbConnectionString => {
                    StrategyKind::Redact
                }
                PiiType::Email
                | PiiType::ChinesePhone
                | PiiType::InternationalPhone
                | PiiType::ChineseIdCard => StrategyKind::StaticMask,
                // 地理/设备标识 → StaticMask
                PiiType::GeoCoordinate
                | PiiType::PhysicalAddress
                | PiiType::MacAddress
                | PiiType::Imei => StrategyKind::StaticMask,
                PiiType::BankCard => StrategyKind::PartialMask,
                // 加密货币/JWT → PartialMask
                PiiType::CryptoAddress | PiiType::JwtToken => StrategyKind::PartialMask,
                // VIN/IBAN/IPv6 → Tokenize
                PiiType::Vin | PiiType::Iban | PiiType::Ipv6Address => StrategyKind::Tokenize,
                _ => StrategyKind::StaticMask,
            },

            // L1 普通 + 任何 PII → Tokenize（可逆）
            SensitivityLevel::Normal => StrategyKind::Tokenize,

            // L0 公开 → 不脱敏（但如果有实体仍用 Tokenize 兜底）
            SensitivityLevel::Public => StrategyKind::Tokenize,
        }
    }

    /// 对文本执行完整的分级 + 策略分派
    pub fn classify_and_resolve(&self, text: &str, entities: &[PiiEntity]) -> Vec<PolicyDecision> {
        let classification = DataClassifier::classify(text);
        self.resolve(&classification, entities)
    }
}

impl Default for SanitizationPolicy {
    fn default() -> Self {
        Self::new(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::classification::DataClassification;

    fn make_entity(text: &str, pii_type: PiiType) -> PiiEntity {
        PiiEntity {
            entity_type: pii_type,
            start: 0,
            end: text.len(),
            text: text.to_string(),
            confidence: 0.95,
        }
    }

    fn make_classification(level: SensitivityLevel) -> DataClassification {
        DataClassification {
            level,
            detected_patterns: vec![],
            confidence: 0.9,
        }
    }

    #[test]
    fn l3_always_redacts() {
        let policy = SanitizationPolicy::new(true);
        let cls = make_classification(SensitivityLevel::Confidential);
        let entities = vec![make_entity("test@mail.com", PiiType::Email)];
        let decisions = policy.resolve(&cls, &entities);
        assert_eq!(decisions[0].strategy, StrategyKind::Redact);
    }

    #[test]
    fn l2_password_redacts() {
        let policy = SanitizationPolicy::new(true);
        let cls = make_classification(SensitivityLevel::Sensitive);
        let entities = vec![make_entity("secret123", PiiType::Password)];
        let decisions = policy.resolve(&cls, &entities);
        assert_eq!(decisions[0].strategy, StrategyKind::Redact);
    }

    #[test]
    fn l2_email_static_masks() {
        let policy = SanitizationPolicy::new(true);
        let cls = make_classification(SensitivityLevel::Sensitive);
        let entities = vec![make_entity("a@b.com", PiiType::Email)];
        let decisions = policy.resolve(&cls, &entities);
        assert_eq!(decisions[0].strategy, StrategyKind::StaticMask);
    }

    #[test]
    fn l2_bank_card_partial_masks() {
        let policy = SanitizationPolicy::new(true);
        let cls = make_classification(SensitivityLevel::Sensitive);
        let entities = vec![make_entity("6222021234567890", PiiType::BankCard)];
        let decisions = policy.resolve(&cls, &entities);
        assert_eq!(decisions[0].strategy, StrategyKind::PartialMask);
    }

    #[test]
    fn l1_tokenizes() {
        let policy = SanitizationPolicy::new(true);
        let cls = make_classification(SensitivityLevel::Normal);
        let entities = vec![make_entity("test@mail.com", PiiType::Email)];
        let decisions = policy.resolve(&cls, &entities);
        assert_eq!(decisions[0].strategy, StrategyKind::Tokenize);
    }

    #[test]
    fn escalation_with_3_types() {
        let policy = SanitizationPolicy::new(true);
        let cls = make_classification(SensitivityLevel::Normal);
        let entities = vec![
            make_entity("a@b.com", PiiType::Email),
            make_entity("13800000000", PiiType::ChinesePhone),
            make_entity("192.168.1.1", PiiType::IpAddress),
        ];
        let decisions = policy.resolve(&cls, &entities);
        // Normal + 3 types → escalated to Sensitive
        // Email at L2 → StaticMask
        assert_eq!(decisions[0].strategy, StrategyKind::StaticMask);
    }

    #[test]
    fn disabled_policy_always_tokenizes() {
        let policy = SanitizationPolicy::new(false);
        let cls = make_classification(SensitivityLevel::Confidential);
        let entities = vec![make_entity("secret", PiiType::Password)];
        let decisions = policy.resolve(&cls, &entities);
        assert_eq!(decisions[0].strategy, StrategyKind::Tokenize);
    }
}
