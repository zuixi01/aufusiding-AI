//! 审查结果与打回机制

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::comm::protocol::Severity;
use crate::agent::types::AgentId;

/// 审查类别
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCategory {
    Security,
    Performance,
    Style,
    Logic,
    Test,
    Architecture,
    Documentation,
}

/// 审查评论（扩展版，含位置信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub severity: Severity,
    pub category: ReviewCategory,
    pub location: String,
    pub message: String,
    pub suggestion: Option<String>,
}

impl ReviewComment {
    pub fn new(
        severity: Severity,
        category: ReviewCategory,
        location: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            category,
            location: location.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn is_blocking(&self) -> bool {
        matches!(self.severity, Severity::Critical | Severity::High)
    }
}

/// 单次审查结果
#[derive(Debug, Clone)]
pub struct ReviewResult {
    pub gate_name: String,
    pub reviewer: AgentId,
    pub approved: bool,
    pub comments: Vec<ReviewComment>,
    pub reviewed_at: DateTime<Utc>,
}

impl ReviewResult {
    pub fn approve(gate_name: impl Into<String>, reviewer: AgentId) -> Self {
        Self {
            gate_name: gate_name.into(),
            reviewer,
            approved: true,
            comments: vec![],
            reviewed_at: Utc::now(),
        }
    }

    pub fn reject(
        gate_name: impl Into<String>,
        reviewer: AgentId,
        comments: Vec<ReviewComment>,
    ) -> Self {
        Self {
            gate_name: gate_name.into(),
            reviewer,
            approved: false,
            comments,
            reviewed_at: Utc::now(),
        }
    }

    /// 统计各严重程度的评论数
    pub fn count_by_severity(&self, severity: &Severity) -> usize {
        self.comments
            .iter()
            .filter(|c| &c.severity == severity)
            .count()
    }

    /// 是否有阻断性问题
    pub fn has_blocking_issues(&self) -> bool {
        self.comments.iter().any(|c| c.is_blocking())
    }
}

/// 打回记录 — 追踪交付物被打回的历史
#[derive(Debug, Clone)]
pub struct RejectionRecord {
    pub deliverable_id: String,
    pub rejections: Vec<ReviewResult>,
}

impl RejectionRecord {
    pub fn new(deliverable_id: impl Into<String>) -> Self {
        Self {
            deliverable_id: deliverable_id.into(),
            rejections: vec![],
        }
    }

    pub fn add_rejection(&mut self, result: ReviewResult) {
        self.rejections.push(result);
    }

    /// 被打回次数
    pub fn rejection_count(&self) -> usize {
        self.rejections.len()
    }

    /// 根据打回次数决定下一步动作
    pub fn next_action(&self, max_rejections: u32) -> RejectionAction {
        let count = self.rejection_count() as u32;
        if count == 0 {
            RejectionAction::None
        } else if count < max_rejections - 1 {
            RejectionAction::FixAndResubmit
        } else if count == max_rejections - 1 {
            RejectionAction::PairReview
        } else {
            RejectionAction::EscalateToRoundtable
        }
    }
}

/// 打回后的动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectionAction {
    /// 无需动作
    None,
    /// 原 Agent 修复后重新提交
    FixAndResubmit,
    /// 触发 Pair Review
    PairReview,
    /// 升级到圆桌会议
    EscalateToRoundtable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_comment_blocking() {
        let critical = ReviewComment::new(
            Severity::Critical,
            ReviewCategory::Security,
            "auth.rs:42",
            "SQL injection",
        );
        assert!(critical.is_blocking());

        let info = ReviewComment::new(
            Severity::Info,
            ReviewCategory::Style,
            "main.rs:1",
            "考虑重命名",
        );
        assert!(!info.is_blocking());
    }

    #[test]
    fn review_comment_with_suggestion() {
        let c = ReviewComment::new(
            Severity::Medium,
            ReviewCategory::Logic,
            "lib.rs:10",
            "边界条件未处理",
        )
        .with_suggestion("添加 if len == 0 的检查");
        assert_eq!(c.suggestion, Some("添加 if len == 0 的检查".into()));
    }

    #[test]
    fn review_result_approve() {
        let r = ReviewResult::approve("functional", AgentId::new());
        assert!(r.approved);
        assert!(!r.has_blocking_issues());
    }

    #[test]
    fn review_result_reject_with_counts() {
        let comments = vec![
            ReviewComment::new(
                Severity::Critical,
                ReviewCategory::Security,
                "a.rs:1",
                "vuln",
            ),
            ReviewComment::new(Severity::Medium, ReviewCategory::Style, "b.rs:2", "style"),
            ReviewComment::new(Severity::Critical, ReviewCategory::Logic, "c.rs:3", "bug"),
        ];
        let r = ReviewResult::reject("security", AgentId::new(), comments);
        assert!(!r.approved);
        assert!(r.has_blocking_issues());
        assert_eq!(r.count_by_severity(&Severity::Critical), 2);
        assert_eq!(r.count_by_severity(&Severity::Medium), 1);
    }

    #[test]
    fn rejection_record_actions() {
        let mut record = RejectionRecord::new("d-1");
        assert_eq!(record.next_action(3), RejectionAction::None);

        record.add_rejection(ReviewResult::reject("gate", AgentId::new(), vec![]));
        assert_eq!(record.next_action(3), RejectionAction::FixAndResubmit);

        record.add_rejection(ReviewResult::reject("gate", AgentId::new(), vec![]));
        assert_eq!(record.next_action(3), RejectionAction::PairReview);

        record.add_rejection(ReviewResult::reject("gate", AgentId::new(), vec![]));
        assert_eq!(record.next_action(3), RejectionAction::EscalateToRoundtable);
    }
}
