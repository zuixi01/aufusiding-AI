//! 冲突解决策略

use super::voting::VoteResult;

/// 冲突解决结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// 方案被采纳
    Adopted,
    /// 大管家裁决
    StewardDecision,
    /// 需要重新投票（否决后解决安全顾虑）
    Revote,
    /// 升级给用户
    EscalateToUser,
}

/// 冲突解决策略
pub struct ResolutionStrategy {
    /// 最大投票轮数
    pub max_voting_rounds: u32,
}

impl ResolutionStrategy {
    pub fn new() -> Self {
        Self {
            max_voting_rounds: 3,
        }
    }

    /// 根据投票结果和当前轮数决定下一步
    pub fn resolve(&self, vote_result: &VoteResult, current_round: u32) -> Resolution {
        match vote_result {
            VoteResult::Adopted { .. } => Resolution::Adopted,
            VoteResult::Vetoed { .. } => {
                if current_round < self.max_voting_rounds {
                    Resolution::Revote
                } else {
                    Resolution::EscalateToUser
                }
            }
            VoteResult::NoConsensus { .. } => {
                if current_round < self.max_voting_rounds {
                    Resolution::StewardDecision
                } else {
                    Resolution::EscalateToUser
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::AgentId;

    #[test]
    fn adopted_resolves_immediately() {
        let strategy = ResolutionStrategy::new();
        let result = VoteResult::Adopted { score: 0.8 };
        assert_eq!(strategy.resolve(&result, 1), Resolution::Adopted);
    }

    #[test]
    fn vetoed_triggers_revote() {
        let strategy = ResolutionStrategy::new();
        let result = VoteResult::Vetoed {
            by: vec![AgentId::new()],
        };
        assert_eq!(strategy.resolve(&result, 1), Resolution::Revote);
    }

    #[test]
    fn vetoed_at_max_rounds_escalates() {
        let strategy = ResolutionStrategy::new();
        let result = VoteResult::Vetoed {
            by: vec![AgentId::new()],
        };
        assert_eq!(strategy.resolve(&result, 3), Resolution::EscalateToUser);
    }

    #[test]
    fn no_consensus_steward_decides() {
        let strategy = ResolutionStrategy::new();
        let result = VoteResult::NoConsensus { score: 0.4 };
        assert_eq!(strategy.resolve(&result, 1), Resolution::StewardDecision);
    }

    #[test]
    fn no_consensus_at_max_rounds_escalates() {
        let strategy = ResolutionStrategy::new();
        let result = VoteResult::NoConsensus { score: 0.3 };
        assert_eq!(strategy.resolve(&result, 3), Resolution::EscalateToUser);
    }
}
