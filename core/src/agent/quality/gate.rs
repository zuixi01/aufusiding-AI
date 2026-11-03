//! 三道质量门禁流程控制
//!
//! 交付物必须依次通过：功能验收 → 安全审查 → 架构审查

use crate::agent::types::{Deliverable, Role};

use super::review::{RejectionAction, RejectionRecord, ReviewResult};

/// 门禁类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateKind {
    /// 功能验收 — QA Agent
    Functional,
    /// 安全审查 — Security Agent
    Security,
    /// 架构审查 — Architect Agent
    Architectural,
}

impl GateKind {
    /// 该门禁由哪个角色负责
    pub fn reviewer_role(&self) -> Role {
        match self {
            Self::Functional => Role::QA,
            Self::Security => Role::Security,
            Self::Architectural => Role::Architect,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Functional => "functional",
            Self::Security => "security",
            Self::Architectural => "architectural",
        }
    }
}

/// 门禁流水线 — 管理三道门禁的顺序执行
pub struct GatePipeline {
    gates: Vec<GateKind>,
    max_rejections: u32,
}

impl GatePipeline {
    /// 创建默认的三道门禁流水线
    pub fn new() -> Self {
        Self {
            gates: vec![
                GateKind::Functional,
                GateKind::Security,
                GateKind::Architectural,
            ],
            max_rejections: 3,
        }
    }

    pub fn with_max_rejections(mut self, max: u32) -> Self {
        self.max_rejections = max;
        self
    }

    /// 获取门禁列表
    pub fn gates(&self) -> &[GateKind] {
        &self.gates
    }

    /// 提交交付物到门禁流水线，返回审查结果
    pub fn submit(&self, _deliverable: &Deliverable, reviews: Vec<ReviewResult>) -> GateOutcome {
        let mut passed_gates = Vec::new();

        for (i, gate) in self.gates.iter().enumerate() {
            if let Some(review) = reviews.get(i) {
                if review.approved {
                    passed_gates.push(gate.clone());
                } else {
                    return GateOutcome::Rejected {
                        failed_gate: gate.clone(),
                        passed_gates,
                        review: review.clone(),
                    };
                }
            } else {
                return GateOutcome::Pending {
                    next_gate: gate.clone(),
                    passed_gates,
                };
            }
        }

        GateOutcome::Approved { passed_gates }
    }

    /// 根据打回记录决定下一步动作
    pub fn rejection_action(&self, record: &RejectionRecord) -> RejectionAction {
        record.next_action(self.max_rejections)
    }
}

/// 门禁结果
#[derive(Debug, Clone)]
pub enum GateOutcome {
    /// 全部通过
    Approved { passed_gates: Vec<GateKind> },
    /// 被某道门禁打回
    Rejected {
        failed_gate: GateKind,
        passed_gates: Vec<GateKind>,
        review: ReviewResult,
    },
    /// 等待下一道门禁审查
    Pending {
        next_gate: GateKind,
        passed_gates: Vec<GateKind>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::comm::protocol::Severity;
    use crate::agent::quality::review::ReviewCategory;
    use crate::agent::quality::review::ReviewComment;
    use crate::agent::types::{AgentId, DeliverableType};
    use chrono::Utc;

    fn sample_deliverable() -> Deliverable {
        Deliverable {
            id: "d-1".into(),
            title: "认证模块".into(),
            content: "code...".into(),
            deliverable_type: DeliverableType::Code,
            created_by: AgentId::new(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn gate_kind_reviewer_roles() {
        assert_eq!(GateKind::Functional.reviewer_role(), Role::QA);
        assert_eq!(GateKind::Security.reviewer_role(), Role::Security);
        assert_eq!(GateKind::Architectural.reviewer_role(), Role::Architect);
    }

    #[test]
    fn pipeline_all_approved() {
        let pipeline = GatePipeline::new();
        let d = sample_deliverable();
        let reviews = vec![
            ReviewResult::approve("functional", AgentId::new()),
            ReviewResult::approve("security", AgentId::new()),
            ReviewResult::approve("architectural", AgentId::new()),
        ];
        let outcome = pipeline.submit(&d, reviews);
        match outcome {
            GateOutcome::Approved { passed_gates } => {
                assert_eq!(passed_gates.len(), 3);
            }
            _ => panic!("expected Approved"),
        }
    }

    #[test]
    fn pipeline_rejected_at_security() {
        let pipeline = GatePipeline::new();
        let d = sample_deliverable();
        let reviews = vec![
            ReviewResult::approve("functional", AgentId::new()),
            ReviewResult::reject(
                "security",
                AgentId::new(),
                vec![ReviewComment::new(
                    Severity::Critical,
                    ReviewCategory::Security,
                    "auth.rs:10",
                    "SQL injection",
                )],
            ),
        ];
        let outcome = pipeline.submit(&d, reviews);
        match outcome {
            GateOutcome::Rejected {
                failed_gate,
                passed_gates,
                review,
            } => {
                assert_eq!(failed_gate, GateKind::Security);
                assert_eq!(passed_gates.len(), 1);
                assert!(!review.approved);
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn pipeline_pending_next_gate() {
        let pipeline = GatePipeline::new();
        let d = sample_deliverable();
        let reviews = vec![ReviewResult::approve("functional", AgentId::new())];
        let outcome = pipeline.submit(&d, reviews);
        match outcome {
            GateOutcome::Pending {
                next_gate,
                passed_gates,
            } => {
                assert_eq!(next_gate, GateKind::Security);
                assert_eq!(passed_gates.len(), 1);
            }
            _ => panic!("expected Pending"),
        }
    }

    #[test]
    fn pipeline_no_reviews_pending_first() {
        let pipeline = GatePipeline::new();
        let d = sample_deliverable();
        let outcome = pipeline.submit(&d, vec![]);
        match outcome {
            GateOutcome::Pending {
                next_gate,
                passed_gates,
            } => {
                assert_eq!(next_gate, GateKind::Functional);
                assert!(passed_gates.is_empty());
            }
            _ => panic!("expected Pending"),
        }
    }

    #[test]
    fn pipeline_rejection_action_escalation() {
        let pipeline = GatePipeline::new();
        let mut record = RejectionRecord::new("d-1");
        assert_eq!(pipeline.rejection_action(&record), RejectionAction::None);

        record.add_rejection(ReviewResult::reject("g", AgentId::new(), vec![]));
        assert_eq!(
            pipeline.rejection_action(&record),
            RejectionAction::FixAndResubmit
        );

        record.add_rejection(ReviewResult::reject("g", AgentId::new(), vec![]));
        assert_eq!(
            pipeline.rejection_action(&record),
            RejectionAction::PairReview
        );

        record.add_rejection(ReviewResult::reject("g", AgentId::new(), vec![]));
        assert_eq!(
            pipeline.rejection_action(&record),
            RejectionAction::EscalateToRoundtable
        );
    }
}
