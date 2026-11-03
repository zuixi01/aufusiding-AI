//! 身份注入引擎 — 根据角色和任务上下文组装完整 Agent 身份

use std::sync::Arc;

use crate::agent::error::AgentError;
use crate::agent::types::{DecisionScope, QualityStandards, Role, Seniority};

use super::competency::Competency;
use super::vault::IdentityVault;

/// 注入后的完整 Agent 身份
#[derive(Debug, Clone)]
pub struct AgentIdentity {
    pub role: Role,
    pub seniority: Seniority,
    pub persona_prompt: String,
    pub core_competencies: Vec<Competency>,
    pub decision_scope: DecisionScope,
    pub quality_standards: QualityStandards,
    pub can_veto: bool,
    pub collaborates_with: Vec<Role>,
}

/// 身份注入引擎 — 从 Vault 加载模板并组装完整身份
pub struct IdentityInjector {
    vault: Arc<IdentityVault>,
}

impl IdentityInjector {
    pub fn new(vault: Arc<IdentityVault>) -> Self {
        Self { vault }
    }

    /// 根据角色和任务上下文，组装完整 Agent 身份
    pub fn inject(&self, role: &Role, task_context: &str) -> Result<AgentIdentity, AgentError> {
        let template = self.vault.get(role)?;

        let persona_prompt = Self::build_prompt(template.persona_prompt, task_context, template);

        Ok(AgentIdentity {
            role: template.role.clone(),
            seniority: template.default_seniority.clone(),
            persona_prompt,
            core_competencies: template.core_competencies.clone(),
            decision_scope: template.decision_scope.clone(),
            quality_standards: template.quality_standards.clone(),
            can_veto: template.can_veto,
            collaborates_with: template.collaborates_with.clone(),
        })
    }
}

impl IdentityInjector {
    /// 组装完整的 persona prompt
    ///
    /// 格式：
    /// [角色身份] + [任务上下文] + [能力矩阵] + [质量标准]
    fn build_prompt(
        base_prompt: &str,
        task_context: &str,
        template: &super::roles::RoleTemplate,
    ) -> String {
        let competencies: Vec<&str> = template
            .core_competencies
            .iter()
            .map(|c| c.name.as_str())
            .collect();

        format!(
            "[角色身份]\n{base_prompt}\n\n\
             [当前任务]\n{task_context}\n\n\
             [核心能力]\n{competencies}\n\n\
             [质量标准]\n\
             - 最低测试覆盖率: {coverage}%\n\
             - 最大圈复杂度: {complexity}\n\
             - 最大函数行数: {func_lines}\n\
             - 零Critical漏洞: {zero_vuln}\n\
             - 输入验证: {input_val}",
            base_prompt = base_prompt,
            task_context = task_context,
            competencies = competencies.join("、"),
            coverage = template.quality_standards.min_test_coverage,
            complexity = template.quality_standards.max_cyclomatic_complexity,
            func_lines = template.quality_standards.max_function_lines,
            zero_vuln = template.quality_standards.zero_critical_vulnerabilities,
            input_val = template.quality_standards.require_input_validation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_injector() -> IdentityInjector {
        let vault = Arc::new(IdentityVault::new());
        IdentityInjector::new(vault)
    }

    #[test]
    fn inject_architect() {
        let injector = test_injector();
        let identity = injector
            .inject(&Role::Architect, "设计新的缓存系统")
            .unwrap();
        assert_eq!(identity.role, Role::Architect);
        assert_eq!(identity.seniority, Seniority::Principal);
        assert!(identity.can_veto);
        assert!(identity.persona_prompt.contains("架构师"));
        assert!(identity.persona_prompt.contains("缓存系统"));
    }

    #[test]
    fn inject_all_roles() {
        let injector = test_injector();
        let roles = [
            Role::Architect,
            Role::Engineer,
            Role::Security,
            Role::QA,
            Role::ProductManager,
            Role::Researcher,
            Role::DevOps,
            Role::DataAnalyst,
        ];
        for role in &roles {
            let identity = injector.inject(role, "test task").unwrap();
            assert_eq!(&identity.role, role);
            assert!(!identity.persona_prompt.is_empty());
            assert!(!identity.core_competencies.is_empty());
        }
    }

    #[test]
    fn inject_prompt_contains_task_context() {
        let injector = test_injector();
        let ctx = "实现用户认证模块";
        let identity = injector.inject(&Role::Engineer, ctx).unwrap();
        assert!(identity.persona_prompt.contains(ctx));
    }

    #[test]
    fn inject_nonexistent_role_fails() {
        let injector = test_injector();
        let result = injector.inject(&Role::Other("ghost".into()), "task");
        assert!(result.is_err());
    }

    #[test]
    fn inject_security_has_veto() {
        let injector = test_injector();
        let identity = injector.inject(&Role::Security, "审查代码").unwrap();
        assert!(identity.can_veto);
        assert_eq!(identity.decision_scope, DecisionScope::Veto);
    }
}
