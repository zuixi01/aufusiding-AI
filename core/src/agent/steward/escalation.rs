//! 升级策略 — 定义何时触发圆桌会议或升级给用户

use crate::agent::comm::protocol::EscalationReason;

/// 升级策略配置
#[derive(Debug, Clone)]
pub struct EscalationPolicy {
    /// 信心阈值，低于此值触发升级
    pub confidence_threshold: f64,
    /// 最大失败次数
    pub max_failures: u32,
    /// 最大打回次数（超过后升级到圆桌会议）
    pub max_rejections: u32,
}

impl EscalationPolicy {
    pub fn default_policy() -> Self {
        Self {
            confidence_threshold: 0.7,
            max_failures: 3,
            max_rejections: 3,
        }
    }

    /// 判断是否应该升级
    pub fn should_escalate(&self, reason: &EscalationReason) -> EscalationAction {
        match reason {
            EscalationReason::LowConfidence { score } => {
                if *score < self.confidence_threshold {
                    EscalationAction::Roundtable
                } else {
                    EscalationAction::None
                }
            }
            EscalationReason::RepeatedFailure { attempts } => {
                if *attempts >= self.max_failures {
                    EscalationAction::Roundtable
                } else {
                    EscalationAction::Retry
                }
            }
            EscalationReason::ConflictingProposals => EscalationAction::Roundtable,
            EscalationReason::Timeout => EscalationAction::Roundtable,
            EscalationReason::SecurityConcern => EscalationAction::Roundtable,
            EscalationReason::NeedHumanDecision => EscalationAction::EscalateToUser,
        }
    }
}

/// 升级动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscalationAction {
    /// 无需升级
    None,
    /// 重试
    Retry,
    /// 启动圆桌会议
    Roundtable,
    /// 升级给用户
    EscalateToUser,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_confidence_triggers_roundtable() {
        let policy = EscalationPolicy::default_policy();
        let reason = EscalationReason::LowConfidence { score: 0.5 };
        assert_eq!(
            policy.should_escalate(&reason),
            EscalationAction::Roundtable
        );
    }

    #[test]
    fn high_confidence_no_escalation() {
        let policy = EscalationPolicy::default_policy();
        let reason = EscalationReason::LowConfidence { score: 0.9 };
        assert_eq!(policy.should_escalate(&reason), EscalationAction::None);
    }

    #[test]
    fn repeated_failure_below_threshold_retries() {
        let policy = EscalationPolicy::default_policy();
        let reason = EscalationReason::RepeatedFailure { attempts: 2 };
        assert_eq!(policy.should_escalate(&reason), EscalationAction::Retry);
    }

    #[test]
    fn repeated_failure_at_threshold_escalates() {
        let policy = EscalationPolicy::default_policy();
        let reason = EscalationReason::RepeatedFailure { attempts: 3 };
        assert_eq!(
            policy.should_escalate(&reason),
            EscalationAction::Roundtable
        );
    }

    #[test]
    fn conflicting_proposals_triggers_roundtable() {
        let policy = EscalationPolicy::default_policy();
        assert_eq!(
            policy.should_escalate(&EscalationReason::ConflictingProposals),
            EscalationAction::Roundtable
        );
    }

    #[test]
    fn security_concern_triggers_roundtable() {
        let policy = EscalationPolicy::default_policy();
        assert_eq!(
            policy.should_escalate(&EscalationReason::SecurityConcern),
            EscalationAction::Roundtable
        );
    }

    #[test]
    fn need_human_decision_escalates_to_user() {
        let policy = EscalationPolicy::default_policy();
        assert_eq!(
            policy.should_escalate(&EscalationReason::NeedHumanDecision),
            EscalationAction::EscalateToUser
        );
    }

    #[test]
    fn timeout_triggers_roundtable() {
        let policy = EscalationPolicy::default_policy();
        assert_eq!(
            policy.should_escalate(&EscalationReason::Timeout),
            EscalationAction::Roundtable
        );
    }
}
