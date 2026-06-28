//! Agent 集群基础类型系统

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// ─── AgentId ──────────────────────────────────────────────

/// Agent 唯一标识符
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(String);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ─── Role ─────────────────────────────────────────────────

/// 岗位角色枚举 — 8种核心角色 + 兜底
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Role {
    Architect,
    Engineer,
    Security,
    QA,
    ProductManager,
    Researcher,
    DevOps,
    DataAnalyst,
    /// 兜底
    Other(String),
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "architect" => Self::Architect,
            "engineer" | "coder" => Self::Engineer,
            "security" => Self::Security,
            "qa" | "quality" => Self::QA,
            "product_manager" | "pm" => Self::ProductManager,
            "researcher" | "research" => Self::Researcher,
            "devops" | "sre" => Self::DevOps,
            "data_analyst" | "analyst" => Self::DataAnalyst,
            other => Self::Other(other.to_string()),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(s) => write!(f, "{}", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

// ─── Seniority ────────────────────────────────────────────

/// 资历级别
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Seniority {
    /// 独立完成复杂任务
    Senior,
    /// 跨领域影响力
    Staff,
    /// 公司级技术方向
    Principal,
}

impl fmt::Display for Seniority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

// ─── DecisionScope ────────────────────────────────────────

/// 决策权限范围
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionScope {
    /// 实现细节决策
    Implementation,
    /// 模块级架构决策
    Module,
    /// 系统级架构决策
    System,
    /// 一票否决权
    Veto,
}

// ─── Priority ─────────────────────────────────────────────

/// 任务优先级
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Priority {
    P0,
    P1,
    P2,
}

// ─── Complexity ───────────────────────────────────────────

/// 任务复杂度
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Complexity {
    Low,
    Medium,
    High,
    Critical,
}

// ─── AgentState ───────────────────────────────────────────

/// Agent 运行状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Working { task_id: String },
    Reviewing { deliverable_id: String },
    Blocked { reason: String },
    Done,
}

// ─── Deliverable ──────────────────────────────────────────

/// 交付物类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableType {
    PRD,
    DesignDoc,
    Code,
    TestSuite,
    SecurityReport,
    Review,
}

/// Agent 交付物
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deliverable {
    pub id: String,
    pub title: String,
    pub content: String,
    pub deliverable_type: DeliverableType,
    pub created_by: AgentId,
    pub created_at: DateTime<Utc>,
}

// ─── QualityStandards ─────────────────────────────────────

/// 质量标准
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityStandards {
    pub min_test_coverage: f64,
    pub max_cyclomatic_complexity: u32,
    pub max_function_lines: u32,
    pub max_file_lines: u32,
    pub zero_critical_vulnerabilities: bool,
    pub require_input_validation: bool,
}

impl QualityStandards {
    /// 工程类角色的默认质量标准
    pub fn engineering_default() -> Self {
        Self {
            min_test_coverage: 80.0,
            max_cyclomatic_complexity: 10,
            max_function_lines: 50,
            max_file_lines: 800,
            zero_critical_vulnerabilities: true,
            require_input_validation: true,
        }
    }

    /// 产品/研究类角色的默认质量标准
    pub fn research_default() -> Self {
        Self {
            min_test_coverage: 60.0,
            max_cyclomatic_complexity: 15,
            max_function_lines: 80,
            max_file_lines: 1000,
            zero_critical_vulnerabilities: true,
            require_input_validation: false,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_uniqueness() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn agent_id_display() {
        let id = AgentId::new();
        let s = id.to_string();
        assert!(!s.is_empty());
        assert_eq!(s, id.as_str());
    }

    #[test]
    fn role_from_str() {
        assert_eq!(Role::from("architect"), Role::Architect);
        assert_eq!(Role::from("ENGINEER"), Role::Engineer);
        assert_eq!(Role::from("coder"), Role::Engineer);
        assert_eq!(Role::from("pm"), Role::ProductManager);
        assert_eq!(Role::from("sre"), Role::DevOps);
        assert_eq!(Role::from("analyst"), Role::DataAnalyst);
        assert_eq!(Role::from("unknown"), Role::Other("unknown".to_string()));
    }

    #[test]
    fn role_display() {
        assert_eq!(Role::Architect.to_string(), "Architect");
        assert_eq!(Role::Other("custom".into()).to_string(), "custom");
    }

    #[test]
    fn role_serde_roundtrip() {
        let role = Role::Security;
        let json = serde_json::to_string(&role).unwrap();
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role, back);
    }

    #[test]
    fn seniority_serde_roundtrip() {
        let s = Seniority::Principal;
        let json = serde_json::to_string(&s).unwrap();
        let back: Seniority = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn agent_state_serde_roundtrip() {
        let state = AgentState::Working {
            task_id: "task-1".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: AgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }

    #[test]
    fn quality_standards_defaults() {
        let eng = QualityStandards::engineering_default();
        assert!((eng.min_test_coverage - 80.0).abs() < f64::EPSILON);
        assert!(eng.zero_critical_vulnerabilities);

        let res = QualityStandards::research_default();
        assert!((res.min_test_coverage - 60.0).abs() < f64::EPSILON);
    }
}
