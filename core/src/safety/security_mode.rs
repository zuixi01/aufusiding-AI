//! 用户安全模式
//!
//! 三档安全模式 (偏执/平衡/便捷) + 策略执行器。
//! 根据数据分级结果决定路由策略。

use serde::{Deserialize, Serialize};

use super::classification::DataClassification;

/// 安全模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SecurityMode {
    /// 偏执模式: 100% 本地，零数据外传
    Paranoid,
    /// 平衡模式: 智能分级，敏感数据本地，普通数据可上云
    #[default]
    Balanced,
    /// 便捷模式: 优先云端，敏感数据匿名化后上云，机密数据阻止
    Convenient,
}

/// 安全策略
#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub mode: SecurityMode,
    /// 按 L0-L3 索引，是否允许云端路由
    pub allow_cloud: [bool; 4],
    /// 按 L0-L3 索引，是否要求匿名化
    pub require_anonymization: [bool; 4],
    /// 按 L0-L3 索引，是否完全阻止
    pub block: [bool; 4],
}

/// 策略决策
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    /// 允许通过
    Allow,
    /// 强制本地路由
    ForceLocal(String),
    /// 要求匿名化后再发送
    RequireAnonymization(String),
    /// 完全阻止
    Block(String),
}

impl SecurityPolicy {
    /// 根据安全模式构建策略
    pub fn from_mode(mode: SecurityMode) -> Self {
        match mode {
            SecurityMode::Paranoid => Self {
                mode,
                allow_cloud: [false, false, false, false],
                require_anonymization: [false, false, false, false],
                block: [false, false, false, false],
            },
            SecurityMode::Balanced => Self {
                mode,
                allow_cloud: [true, true, false, false],
                require_anonymization: [false, false, false, false],
                block: [false, false, false, false],
            },
            SecurityMode::Convenient => Self {
                mode,
                allow_cloud: [true, true, false, false],
                require_anonymization: [false, false, true, false],
                block: [false, false, false, true],
            },
        }
    }
}

/// 安全策略执行器
pub struct SecurityPolicyEnforcer {
    policy: SecurityPolicy,
}

impl SecurityPolicyEnforcer {
    pub fn new(mode: SecurityMode) -> Self {
        Self {
            policy: SecurityPolicy::from_mode(mode),
        }
    }

    /// 获取当前安全模式
    pub fn mode(&self) -> SecurityMode {
        self.policy.mode
    }

    /// 根据数据分级和路由目标执行策略
    pub fn enforce(
        &self,
        classification: &DataClassification,
        is_cloud_route: bool,
    ) -> PolicyDecision {
        // 本地路由始终允许
        if !is_cloud_route {
            return PolicyDecision::Allow;
        }

        let idx = classification.level as usize;

        // 检查是否阻止
        if self.policy.block[idx] {
            return PolicyDecision::Block(format!(
                "{:?}模式下，{:?}级别数据禁止发送到云端",
                self.policy.mode, classification.level
            ));
        }

        // 检查是否要求匿名化
        if self.policy.require_anonymization[idx] {
            return PolicyDecision::RequireAnonymization(format!(
                "{:?}模式下，{:?}级别数据需要匿名化后才能发送到云端",
                self.policy.mode, classification.level
            ));
        }

        // 检查是否允许云端
        if !self.policy.allow_cloud[idx] {
            return PolicyDecision::ForceLocal(format!(
                "{:?}模式下，{:?}级别数据必须在本地处理",
                self.policy.mode, classification.level
            ));
        }

        PolicyDecision::Allow
    }
}

impl SecurityMode {
    /// 从配置字符串解析
    pub fn from_str_config(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "paranoid" | "偏执" => SecurityMode::Paranoid,
            "convenient" | "便捷" => SecurityMode::Convenient,
            _ => SecurityMode::Balanced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::classification::SensitivityLevel;
    use super::*;

    fn make_classification(level: SensitivityLevel) -> DataClassification {
        DataClassification {
            level,
            detected_patterns: vec![],
            confidence: 0.9,
        }
    }

    // ── Paranoid 模式 ──

    #[test]
    fn paranoid_forces_local_for_public() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Paranoid);
        let class = make_classification(SensitivityLevel::Public);
        assert!(matches!(
            enforcer.enforce(&class, true),
            PolicyDecision::ForceLocal(_)
        ));
    }

    #[test]
    fn paranoid_forces_local_for_normal() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Paranoid);
        let class = make_classification(SensitivityLevel::Normal);
        assert!(matches!(
            enforcer.enforce(&class, true),
            PolicyDecision::ForceLocal(_)
        ));
    }

    #[test]
    fn paranoid_forces_local_for_confidential() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Paranoid);
        let class = make_classification(SensitivityLevel::Confidential);
        assert!(matches!(
            enforcer.enforce(&class, true),
            PolicyDecision::ForceLocal(_)
        ));
    }

    // ── Balanced 模式 ──

    #[test]
    fn balanced_allows_cloud_for_public() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Balanced);
        let class = make_classification(SensitivityLevel::Public);
        assert_eq!(enforcer.enforce(&class, true), PolicyDecision::Allow);
    }

    #[test]
    fn balanced_allows_cloud_for_normal() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Balanced);
        let class = make_classification(SensitivityLevel::Normal);
        assert_eq!(enforcer.enforce(&class, true), PolicyDecision::Allow);
    }

    #[test]
    fn balanced_forces_local_for_sensitive() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Balanced);
        let class = make_classification(SensitivityLevel::Sensitive);
        assert!(matches!(
            enforcer.enforce(&class, true),
            PolicyDecision::ForceLocal(_)
        ));
    }

    // ── Convenient 模式 ──

    #[test]
    fn convenient_allows_cloud_for_normal() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Convenient);
        let class = make_classification(SensitivityLevel::Normal);
        assert_eq!(enforcer.enforce(&class, true), PolicyDecision::Allow);
    }

    #[test]
    fn convenient_anonymizes_sensitive() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Convenient);
        let class = make_classification(SensitivityLevel::Sensitive);
        assert!(matches!(
            enforcer.enforce(&class, true),
            PolicyDecision::RequireAnonymization(_)
        ));
    }

    #[test]
    fn convenient_blocks_confidential() {
        let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Convenient);
        let class = make_classification(SensitivityLevel::Confidential);
        assert!(matches!(
            enforcer.enforce(&class, true),
            PolicyDecision::Block(_)
        ));
    }

    // ── 通用 ──

    #[test]
    fn local_route_always_allowed() {
        for mode in [
            SecurityMode::Paranoid,
            SecurityMode::Balanced,
            SecurityMode::Convenient,
        ] {
            let enforcer = SecurityPolicyEnforcer::new(mode);
            for level in [SensitivityLevel::Public, SensitivityLevel::Confidential] {
                let class = make_classification(level);
                assert_eq!(enforcer.enforce(&class, false), PolicyDecision::Allow);
            }
        }
    }

    #[test]
    fn default_mode_is_balanced() {
        assert_eq!(SecurityMode::default(), SecurityMode::Balanced);
    }

    #[test]
    fn from_str_config_parses_modes() {
        assert_eq!(
            SecurityMode::from_str_config("paranoid"),
            SecurityMode::Paranoid
        );
        assert_eq!(
            SecurityMode::from_str_config("convenient"),
            SecurityMode::Convenient
        );
        assert_eq!(
            SecurityMode::from_str_config("balanced"),
            SecurityMode::Balanced
        );
        assert_eq!(
            SecurityMode::from_str_config("unknown"),
            SecurityMode::Balanced
        );
        assert_eq!(
            SecurityMode::from_str_config("偏执"),
            SecurityMode::Paranoid
        );
    }
}
