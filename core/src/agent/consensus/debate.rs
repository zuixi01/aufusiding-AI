//! 辩论流程 — 提案、质疑、回应

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::agent::types::AgentId;

/// 提案
#[derive(Debug, Clone)]
pub struct Proposal {
    pub id: Uuid,
    pub author: AgentId,
    pub content: String,
    pub rationale: String,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub challenges: Vec<Challenge>,
}

impl Proposal {
    pub fn new(
        author: AgentId,
        content: impl Into<String>,
        rationale: impl Into<String>,
        confidence: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            author,
            content: content.into(),
            rationale: rationale.into(),
            confidence: confidence.clamp(0.0, 1.0),
            created_at: Utc::now(),
            challenges: vec![],
        }
    }

    pub fn add_challenge(&mut self, challenge: Challenge) {
        self.challenges.push(challenge);
    }

    /// 是否所有质疑都已回应
    pub fn all_challenges_addressed(&self) -> bool {
        self.challenges.iter().all(|c| c.response.is_some())
    }

    /// 未回应的质疑数
    pub fn unaddressed_count(&self) -> usize {
        self.challenges
            .iter()
            .filter(|c| c.response.is_none())
            .count()
    }
}

/// 质疑
#[derive(Debug, Clone)]
pub struct Challenge {
    pub id: Uuid,
    pub challenger: AgentId,
    pub concern: String,
    pub response: Option<ChallengeResponse>,
    pub created_at: DateTime<Utc>,
}

impl Challenge {
    pub fn new(challenger: AgentId, concern: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            challenger,
            concern: concern.into(),
            response: None,
            created_at: Utc::now(),
        }
    }

    pub fn respond(&mut self, response: ChallengeResponse) {
        self.response = Some(response);
    }
}

/// 质疑回应
#[derive(Debug, Clone)]
pub struct ChallengeResponse {
    pub content: String,
    pub modified_proposal: bool,
    pub created_at: DateTime<Utc>,
}

impl ChallengeResponse {
    pub fn new(content: impl Into<String>, modified: bool) -> Self {
        Self {
            content: content.into(),
            modified_proposal: modified,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposal_creation() {
        let p = Proposal::new(AgentId::new(), "用 Redis 做缓存", "性能好", 0.85);
        assert!((p.confidence - 0.85).abs() < f64::EPSILON);
        assert!(p.all_challenges_addressed());
        assert_eq!(p.unaddressed_count(), 0);
    }

    #[test]
    fn proposal_confidence_clamped() {
        let p = Proposal::new(AgentId::new(), "c", "r", 1.5);
        assert!((p.confidence - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn challenge_and_response() {
        let mut p = Proposal::new(AgentId::new(), "方案A", "理由", 0.8);
        let mut c = Challenge::new(AgentId::new(), "高并发下会有问题");
        assert!(!p.all_challenges_addressed() || p.challenges.is_empty());

        p.add_challenge(c.clone());
        assert_eq!(p.unaddressed_count(), 1);
        assert!(!p.all_challenges_addressed());

        c.respond(ChallengeResponse::new("已添加连接池", true));
        p.challenges[0] = c;
        assert!(p.all_challenges_addressed());
    }

    #[test]
    fn multiple_challenges() {
        let mut p = Proposal::new(AgentId::new(), "方案", "理由", 0.7);
        p.add_challenge(Challenge::new(AgentId::new(), "安全问题"));
        p.add_challenge(Challenge::new(AgentId::new(), "性能问题"));
        assert_eq!(p.unaddressed_count(), 2);

        p.challenges[0].respond(ChallengeResponse::new("已修复", false));
        assert_eq!(p.unaddressed_count(), 1);
        assert!(!p.all_challenges_addressed());
    }
}
