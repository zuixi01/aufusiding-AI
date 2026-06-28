//! 加权投票算法
//!
//! vote_weight = 专业相关度 × 信心分数 × 历史准确率

use uuid::Uuid;

use crate::agent::comm::protocol::Stance;
use crate::agent::types::AgentId;

/// 加权投票
#[derive(Debug, Clone)]
pub struct WeightedVote {
    pub agent_id: AgentId,
    pub proposal_id: Uuid,
    pub stance: Stance,
    pub reasoning: String,
    pub weight: f64,
    pub has_veto_power: bool,
}

impl WeightedVote {
    pub fn new(
        agent_id: AgentId,
        proposal_id: Uuid,
        stance: Stance,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            agent_id,
            proposal_id,
            stance,
            reasoning: reasoning.into(),
            weight: 1.0,
            has_veto_power: false,
        }
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_veto_power(mut self) -> Self {
        self.has_veto_power = true;
        self
    }

    /// 计算权重: 专业相关度 × 信心分数 × 历史准确率
    pub fn calculate_weight(relevance: f64, confidence: f64, accuracy: f64) -> f64 {
        (relevance * confidence * accuracy).clamp(0.0, 1.0)
    }
}

/// 投票结果
#[derive(Debug, Clone, PartialEq)]
pub enum VoteResult {
    /// 方案被采纳（≥60% 加权同意）
    Adopted { score: f64 },
    /// 被否决（Security 等有否决权的 Agent）
    Vetoed { by: Vec<AgentId> },
    /// 未达成共识（<60% 且无否决）
    NoConsensus { score: f64 },
}

/// 计算投票结果
pub fn tally_votes(votes: &[WeightedVote]) -> VoteResult {
    if votes.is_empty() {
        return VoteResult::NoConsensus { score: 0.0 };
    }

    let total_weight: f64 = votes.iter().map(|v| v.weight).sum();
    if total_weight == 0.0 {
        return VoteResult::NoConsensus { score: 0.0 };
    }

    // 检查否决
    let vetoes: Vec<AgentId> = votes
        .iter()
        .filter(|v| v.has_veto_power && matches!(v.stance, Stance::StrongDisagree))
        .map(|v| v.agent_id.clone())
        .collect();

    if !vetoes.is_empty() {
        return VoteResult::Vetoed { by: vetoes };
    }

    // 计算加权同意比例
    let agree_weight: f64 = votes
        .iter()
        .filter(|v| matches!(v.stance, Stance::Agree | Stance::StrongAgree))
        .map(|v| v.weight)
        .sum();

    let score = agree_weight / total_weight;

    if score >= 0.6 {
        VoteResult::Adopted { score }
    } else {
        VoteResult::NoConsensus { score }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vote(stance: Stance, weight: f64, veto: bool) -> WeightedVote {
        let mut v = WeightedVote::new(AgentId::new(), Uuid::new_v4(), stance, "reason")
            .with_weight(weight);
        if veto {
            v = v.with_veto_power();
        }
        v
    }

    #[test]
    fn adopted_when_above_threshold() {
        let votes = vec![
            vote(Stance::StrongAgree, 1.0, false),
            vote(Stance::Agree, 1.0, false),
            vote(Stance::Disagree, 0.5, false),
        ];
        match tally_votes(&votes) {
            VoteResult::Adopted { score } => assert!(score >= 0.6),
            _ => panic!("expected Adopted"),
        }
    }

    #[test]
    fn no_consensus_when_below_threshold() {
        let votes = vec![
            vote(Stance::Agree, 0.5, false),
            vote(Stance::Disagree, 1.0, false),
            vote(Stance::StrongDisagree, 1.0, false),
        ];
        match tally_votes(&votes) {
            VoteResult::NoConsensus { score } => assert!(score < 0.6),
            _ => panic!("expected NoConsensus"),
        }
    }

    #[test]
    fn vetoed_by_security() {
        let votes = vec![
            vote(Stance::StrongAgree, 1.0, false),
            vote(Stance::Agree, 1.0, false),
            vote(Stance::StrongDisagree, 1.0, true), // Security veto
        ];
        match tally_votes(&votes) {
            VoteResult::Vetoed { by } => assert_eq!(by.len(), 1),
            _ => panic!("expected Vetoed"),
        }
    }

    #[test]
    fn no_veto_without_power() {
        let votes = vec![
            vote(Stance::StrongAgree, 1.0, false),
            vote(Stance::StrongDisagree, 1.0, false), // no veto power
        ];
        // Should not be vetoed, just NoConsensus
        match tally_votes(&votes) {
            VoteResult::NoConsensus { .. } => {}
            _ => panic!("expected NoConsensus"),
        }
    }

    #[test]
    fn empty_votes_no_consensus() {
        assert_eq!(tally_votes(&[]), VoteResult::NoConsensus { score: 0.0 });
    }

    #[test]
    fn calculate_weight_clamps() {
        assert!((WeightedVote::calculate_weight(0.8, 0.9, 0.95) - 0.684).abs() < 0.01);
        assert!((WeightedVote::calculate_weight(2.0, 2.0, 2.0) - 1.0).abs() < f64::EPSILON);
        assert!((WeightedVote::calculate_weight(0.0, 0.5, 0.5) - 0.0).abs() < f64::EPSILON);
    }
}
