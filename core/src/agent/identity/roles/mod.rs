//! 岗位身份模板 — 8个核心角色

pub mod architect;
pub mod data_analyst;
pub mod devops;
pub mod engineer;
pub mod product_manager;
pub mod qa;
pub mod researcher;
pub mod security;

use super::competency::Competency;
use crate::agent::types::{DecisionScope, QualityStandards, Role, Seniority};

/// 岗位身份模板 — 从 Identity Vault 加载
#[derive(Debug, Clone)]
pub struct RoleTemplate {
    pub role: Role,
    pub default_seniority: Seniority,
    pub persona_prompt: &'static str,
    pub core_competencies: Vec<Competency>,
    pub decision_scope: DecisionScope,
    pub quality_standards: QualityStandards,
    pub can_veto: bool,
    pub collaborates_with: Vec<Role>,
    pub watches: &'static [&'static str],
}
