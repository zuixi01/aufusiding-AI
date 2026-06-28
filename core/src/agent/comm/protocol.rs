//! Agent 通信协议 — 消息类型、信封、过滤器
//!
//! 定义 Agent 之间通信的所有消息结构。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::types::{AgentId, AgentState, Deliverable, Role};

// ─── MessageTarget ────────────────────────────────────────

/// 消息目标
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageTarget {
    /// 发给特定 Agent
    Agent(AgentId),
    /// 发给某个角色（不关心具体是谁）
    Role(Role),
    /// 广播给所有 Agent
    Broadcast,
    /// 发给讨论组
    Group(String),
}

// ─── Stance ───────────────────────────────────────────────

/// 投票立场
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    StrongAgree,
    Agree,
    Neutral,
    Disagree,
    StrongDisagree,
}

// ─── Severity ─────────────────────────────────────────────

/// 审查严重程度
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

// ─── ReviewComment ────────────────────────────────────────

/// 审查评论
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub severity: Severity,
    pub category: String,
    pub message: String,
    pub suggestion: Option<String>,
}

// ─── EscalationReason ─────────────────────────────────────

/// 升级原因
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationReason {
    /// 信心不足
    LowConfidence { score: f64 },
    /// 反复失败
    RepeatedFailure { attempts: u32 },
    /// 方案冲突
    ConflictingProposals,
    /// 超时
    Timeout,
    /// 安全顾虑
    SecurityConcern,
    /// 需要用户决策
    NeedHumanDecision,
}

// ─── AgentMessage ─────────────────────────────────────────

/// Agent 间消息载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessage {
    // ── 任务相关 ──
    TaskAssigned {
        task_id: String,
        title: String,
        description: String,
    },
    TaskCompleted {
        task_id: String,
        deliverable: Deliverable,
    },
    TaskBlocked {
        task_id: String,
        reason: String,
        need_help: bool,
    },

    // ── 审查相关 ──
    ReviewRequest {
        deliverable: Deliverable,
    },
    ReviewResult {
        approved: bool,
        feedback: Vec<ReviewComment>,
        severity: Severity,
    },

    // ── 协作相关 ──
    Question {
        content: String,
    },
    Answer {
        content: String,
        ref_question: Uuid,
    },
    Suggestion {
        content: String,
        confidence: f64,
    },
    TechnicalDiscussion {
        topic: String,
        position: String,
    },

    // ── 共识相关 ──
    DebateStart {
        topic: String,
        context: String,
    },
    Proposal {
        content: String,
        rationale: String,
        confidence: f64,
    },
    Challenge {
        target_proposal: Uuid,
        concern: String,
    },
    Vote {
        proposal_id: Uuid,
        stance: Stance,
        reasoning: String,
    },
    Consensus {
        decision: String,
        dissents: Vec<String>,
    },

    // ── 控制相关 ──
    Escalate {
        reason: EscalationReason,
        context: String,
    },
    Heartbeat {
        status: AgentState,
        progress: f64,
    },
    Shutdown,
}

// ─── AgentEnvelope ────────────────────────────────────────

/// 消息信封 — 包装消息载荷 + 路由信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEnvelope {
    pub id: Uuid,
    pub correlation_id: Option<Uuid>,
    pub from: AgentId,
    pub to: MessageTarget,
    pub payload: AgentMessage,
    pub timestamp: DateTime<Utc>,
}

impl AgentEnvelope {
    pub fn new(from: AgentId, to: MessageTarget, payload: AgentMessage) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id: None,
            from,
            to,
            payload,
            timestamp: Utc::now(),
        }
    }

    pub fn with_correlation(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
}

// ─── MessageFilter ────────────────────────────────────────

/// 消息过滤器 — 用于订阅时过滤感兴趣的消息
#[derive(Debug, Clone)]
pub enum MessageFilter {
    /// 只接收发给自己的消息
    DirectOnly,
    /// 只接收特定目标的消息
    TargetIs(MessageTarget),
    /// 只接收特定类型的消息（按 discriminant 名称匹配）
    MessageKind(String),
    /// 接收所有消息
    All,
}

impl MessageFilter {
    /// 检查信封是否匹配此过滤器
    pub fn matches(&self, envelope: &AgentEnvelope, self_id: &AgentId) -> bool {
        match self {
            Self::DirectOnly => matches!(&envelope.to, MessageTarget::Agent(id) if id == self_id),
            Self::TargetIs(target) => &envelope.to == target,
            Self::MessageKind(kind) => envelope_kind_name(&envelope.payload) == kind,
            Self::All => true,
        }
    }
}

/// 获取 AgentMessage 变体名称（用于 MessageKind 过滤）
fn envelope_kind_name(msg: &AgentMessage) -> &'static str {
    match msg {
        AgentMessage::TaskAssigned { .. } => "task_assigned",
        AgentMessage::TaskCompleted { .. } => "task_completed",
        AgentMessage::TaskBlocked { .. } => "task_blocked",
        AgentMessage::ReviewRequest { .. } => "review_request",
        AgentMessage::ReviewResult { .. } => "review_result",
        AgentMessage::Question { .. } => "question",
        AgentMessage::Answer { .. } => "answer",
        AgentMessage::Suggestion { .. } => "suggestion",
        AgentMessage::TechnicalDiscussion { .. } => "technical_discussion",
        AgentMessage::DebateStart { .. } => "debate_start",
        AgentMessage::Proposal { .. } => "proposal",
        AgentMessage::Challenge { .. } => "challenge",
        AgentMessage::Vote { .. } => "vote",
        AgentMessage::Consensus { .. } => "consensus",
        AgentMessage::Escalate { .. } => "escalate",
        AgentMessage::Heartbeat { .. } => "heartbeat",
        AgentMessage::Shutdown => "shutdown",
    }
}

// ─── Tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_envelope(to: MessageTarget, payload: AgentMessage) -> AgentEnvelope {
        AgentEnvelope::new(AgentId::new(), to, payload)
    }

    #[test]
    fn envelope_new_sets_fields() {
        let from = AgentId::new();
        let env = AgentEnvelope::new(
            from.clone(),
            MessageTarget::Broadcast,
            AgentMessage::Shutdown,
        );
        assert_eq!(env.from, from);
        assert!(env.correlation_id.is_none());
        assert!(env.timestamp <= Utc::now());
    }

    #[test]
    fn envelope_with_correlation() {
        let cid = Uuid::new_v4();
        let env = AgentEnvelope::new(
            AgentId::new(),
            MessageTarget::Broadcast,
            AgentMessage::Shutdown,
        )
        .with_correlation(cid);
        assert_eq!(env.correlation_id, Some(cid));
    }

    #[test]
    fn filter_direct_only() {
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        let filter = MessageFilter::DirectOnly;

        let env_to_a = sample_envelope(
            MessageTarget::Agent(agent_a.clone()),
            AgentMessage::Shutdown,
        );
        let env_broadcast = sample_envelope(MessageTarget::Broadcast, AgentMessage::Shutdown);

        assert!(filter.matches(&env_to_a, &agent_a));
        assert!(!filter.matches(&env_broadcast, &agent_a));
        assert!(!filter.matches(&env_to_a, &agent_b));
    }

    #[test]
    fn filter_message_kind() {
        let me = AgentId::new();
        let filter = MessageFilter::MessageKind("review_request".into());

        let env_review = sample_envelope(
            MessageTarget::Broadcast,
            AgentMessage::ReviewRequest {
                deliverable: Deliverable {
                    id: "d1".into(),
                    title: "test".into(),
                    content: "c".into(),
                    deliverable_type: crate::agent::types::DeliverableType::Code,
                    created_by: AgentId::new(),
                    created_at: Utc::now(),
                },
            },
        );
        let env_shutdown = sample_envelope(MessageTarget::Broadcast, AgentMessage::Shutdown);

        assert!(filter.matches(&env_review, &me));
        assert!(!filter.matches(&env_shutdown, &me));
    }

    #[test]
    fn filter_all_matches_everything() {
        let me = AgentId::new();
        let filter = MessageFilter::All;
        let env = sample_envelope(MessageTarget::Broadcast, AgentMessage::Shutdown);
        assert!(filter.matches(&env, &me));
    }

    #[test]
    fn stance_serde_roundtrip() {
        let s = Stance::StrongDisagree;
        let json = serde_json::to_string(&s).unwrap();
        let back: Stance = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn severity_serde_roundtrip() {
        let s = Severity::Critical;
        let json = serde_json::to_string(&s).unwrap();
        let back: Severity = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn message_target_serde_roundtrip() {
        let t = MessageTarget::Group("roundtable-1".into());
        let json = serde_json::to_string(&t).unwrap();
        let back: MessageTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }
}
