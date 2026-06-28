//! 圆桌会议协议 — 4轮结构化讨论

use uuid::Uuid;

use crate::agent::comm::protocol::{EscalationReason, Severity};
use crate::agent::types::{AgentId, Role};

use super::debate::Proposal;
use super::resolution::{Resolution, ResolutionStrategy};
use super::voting::{tally_votes, VoteResult, WeightedVote};

/// 圆桌会议
#[derive(Debug, Clone)]
pub struct Roundtable {
    pub id: Uuid,
    pub topic: String,
    pub trigger: EscalationReason,
    pub participants: Vec<AgentId>,
    pub problem_statement: Option<String>,
    pub proposals: Vec<Proposal>,
    pub votes: Vec<WeightedVote>,
    pub outcome: Option<ConsensusOutcome>,
}

/// 共识结果
#[derive(Debug, Clone)]
pub enum ConsensusOutcome {
    /// 方案被采纳
    Adopted {
        proposal_id: Uuid,
        vote_score: f64,
        dissents: Vec<Dissent>,
    },
    /// 大管家裁决
    StewardDecision {
        decision: String,
        rationale: String,
    },
    /// 升级给用户
    EscalatedToUser {
        summary: String,
        options: Vec<String>,
    },
}

/// 异议记录
#[derive(Debug, Clone)]
pub struct Dissent {
    pub agent_id: AgentId,
    pub role: Role,
    pub concern: String,
    pub severity: Severity,
}

impl Roundtable {
    /// 创建新的圆桌会议
    pub fn new(
        topic: impl Into<String>,
        trigger: EscalationReason,
        participants: Vec<AgentId>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            topic: topic.into(),
            trigger,
            participants,
            problem_statement: None,
            proposals: Vec::new(),
            votes: Vec::new(),
            outcome: None,
        }
    }

    /// Round 1: 设置问题陈述
    pub fn set_problem_statement(&mut self, statement: impl Into<String>) {
        self.problem_statement = Some(statement.into());
    }

    /// Round 2: 添加提案
    pub fn add_proposal(&mut self, proposal: Proposal) {
        self.proposals.push(proposal);
    }

    /// Round 4: 添加投票
    pub fn add_vote(&mut self, vote: WeightedVote) {
        self.votes.push(vote);
    }

    /// 计算投票结果
    pub fn tally(&self) -> VoteResult {
        tally_votes(&self.votes)
    }

    /// 根据投票结果和解决策略确定最终结果
    pub fn resolve(&mut self, strategy: &ResolutionStrategy, current_round: u32) {
        let vote_result = self.tally();
        let resolution = strategy.resolve(&vote_result, current_round);

        self.outcome = match resolution {
            Resolution::Adopted => {
                // 找到得票最高的提案
                let best_proposal = self.proposals.first().map(|p| p.id);
                let dissents = self.collect_dissents();
                if let (VoteResult::Adopted { score }, Some(pid)) = (&vote_result, best_proposal) {
                    Some(ConsensusOutcome::Adopted {
                        proposal_id: pid,
                        vote_score: *score,
                        dissents,
                    })
                } else {
                    None
                }
            }
            Resolution::StewardDecision => Some(ConsensusOutcome::StewardDecision {
                decision: "大管家裁决".into(),
                rationale: format!("投票未达共识阈值: {:?}", vote_result),
            }),
            Resolution::EscalateToUser => Some(ConsensusOutcome::EscalatedToUser {
                summary: format!("议题: {}", self.topic),
                options: self.proposals.iter().map(|p| p.content.clone()).collect(),
            }),
            Resolution::Revote => None, // 需要重新投票，不设置 outcome
        };
    }

    /// 收集异议
    fn collect_dissents(&self) -> Vec<Dissent> {
        self.votes
            .iter()
            .filter(|v| {
                matches!(
                    v.stance,
                    crate::agent::comm::protocol::Stance::Disagree
                        | crate::agent::comm::protocol::Stance::StrongDisagree
                )
            })
            .map(|v| Dissent {
                agent_id: v.agent_id.clone(),
                role: Role::Other("unknown".into()),
                concern: v.reasoning.clone(),
                severity: Severity::Medium,
            })
            .collect()
    }

    /// 会议是否已结束
    pub fn is_concluded(&self) -> bool {
        self.outcome.is_some()
    }

    /// 参与者数量
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::comm::protocol::Stance;

    fn make_roundtable() -> Roundtable {
        Roundtable::new(
            "缓存策略选择",
            EscalationReason::ConflictingProposals,
            vec![AgentId::new(), AgentId::new(), AgentId::new()],
        )
    }

    #[test]
    fn roundtable_creation() {
        let rt = make_roundtable();
        assert_eq!(rt.participant_count(), 3);
        assert!(!rt.is_concluded());
        assert!(rt.problem_statement.is_none());
    }

    #[test]
    fn set_problem_statement() {
        let mut rt = make_roundtable();
        rt.set_problem_statement("缓存命中率低于50%");
        assert_eq!(rt.problem_statement, Some("缓存命中率低于50%".into()));
    }

    #[test]
    fn add_proposals_and_votes() {
        let mut rt = make_roundtable();
        let p = Proposal::new(AgentId::new(), "用 Redis", "性能好", 0.9);
        let pid = p.id;
        rt.add_proposal(p);

        rt.add_vote(
            WeightedVote::new(AgentId::new(), pid, Stance::StrongAgree, "同意")
                .with_weight(1.0),
        );
        rt.add_vote(
            WeightedVote::new(AgentId::new(), pid, Stance::Agree, "可以")
                .with_weight(0.8),
        );

        match rt.tally() {
            VoteResult::Adopted { score } => assert!(score >= 0.6),
            _ => panic!("expected Adopted"),
        }
    }

    #[test]
    fn resolve_adopted() {
        let mut rt = make_roundtable();
        let p = Proposal::new(AgentId::new(), "方案A", "理由", 0.9);
        let pid = p.id;
        rt.add_proposal(p);
        rt.add_vote(
            WeightedVote::new(AgentId::new(), pid, Stance::StrongAgree, "ok")
                .with_weight(1.0),
        );

        let strategy = ResolutionStrategy::new();
        rt.resolve(&strategy, 1);
        assert!(rt.is_concluded());
        match rt.outcome.as_ref().unwrap() {
            ConsensusOutcome::Adopted { vote_score, .. } => assert!(*vote_score >= 0.6),
            _ => panic!("expected Adopted"),
        }
    }

    #[test]
    fn resolve_escalated_to_user() {
        let mut rt = make_roundtable();
        let p = Proposal::new(AgentId::new(), "方案A", "理由", 0.5);
        let pid = p.id;
        rt.add_proposal(p);
        // Security veto
        rt.add_vote(
            WeightedVote::new(AgentId::new(), pid, Stance::StrongDisagree, "安全问题")
                .with_weight(1.0)
                .with_veto_power(),
        );

        let strategy = ResolutionStrategy::new();
        rt.resolve(&strategy, 3); // max rounds reached
        assert!(rt.is_concluded());
        match rt.outcome.as_ref().unwrap() {
            ConsensusOutcome::EscalatedToUser { .. } => {}
            _ => panic!("expected EscalatedToUser"),
        }
    }

    #[test]
    fn resolve_revote_no_outcome() {
        let mut rt = make_roundtable();
        let p = Proposal::new(AgentId::new(), "方案A", "理由", 0.5);
        let pid = p.id;
        rt.add_proposal(p);
        rt.add_vote(
            WeightedVote::new(AgentId::new(), pid, Stance::StrongDisagree, "安全问题")
                .with_weight(1.0)
                .with_veto_power(),
        );

        let strategy = ResolutionStrategy::new();
        rt.resolve(&strategy, 1); // round 1, should revote
        assert!(!rt.is_concluded());
    }
}
