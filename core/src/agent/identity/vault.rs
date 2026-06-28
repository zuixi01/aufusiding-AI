//! Identity Vault — 岗位身份模板库
//!
//! 加载并管理所有角色的身份模板，供 IdentityInjector 查询。

use std::collections::HashMap;

use crate::agent::error::AgentError;
use crate::agent::types::Role;

use super::roles::RoleTemplate;
use super::roles::{
    architect, data_analyst, devops, engineer, product_manager, qa, researcher, security,
};

/// 岗位身份模板库 — 存储所有角色的身份模板
pub struct IdentityVault {
    templates: HashMap<Role, RoleTemplate>,
}

impl IdentityVault {
    /// 创建并加载所有内置角色模板
    pub fn new() -> Self {
        let mut templates = HashMap::new();
        let all = [
            architect::template(),
            engineer::template(),
            security::template(),
            qa::template(),
            product_manager::template(),
            researcher::template(),
            devops::template(),
            data_analyst::template(),
        ];
        for t in all {
            templates.insert(t.role.clone(), t);
        }
        Self { templates }
    }

    /// 获取指定角色的模板
    pub fn get(&self, role: &Role) -> Result<&RoleTemplate, AgentError> {
        self.templates
            .get(role)
            .ok_or_else(|| AgentError::RoleNotFound(role.to_string()))
    }

    /// 列出所有已注册的角色
    pub fn list_roles(&self) -> Vec<&Role> {
        self.templates.keys().collect()
    }

    /// 检查角色是否存在
    pub fn has_role(&self, role: &Role) -> bool {
        self.templates.contains_key(role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_loads_all_roles() {
        let vault = IdentityVault::new();
        assert_eq!(vault.list_roles().len(), 8);
    }

    #[test]
    fn vault_get_existing_role() {
        let vault = IdentityVault::new();
        let tpl = vault.get(&Role::Architect).unwrap();
        assert_eq!(tpl.role, Role::Architect);
        assert!(!tpl.persona_prompt.is_empty());
    }

    #[test]
    fn vault_get_nonexistent_role() {
        let vault = IdentityVault::new();
        let result = vault.get(&Role::Other("unknown".into()));
        assert!(result.is_err());
    }

    #[test]
    fn vault_has_role() {
        let vault = IdentityVault::new();
        assert!(vault.has_role(&Role::Security));
        assert!(!vault.has_role(&Role::Other("nope".into())));
    }

    #[test]
    fn vault_all_roles_have_competencies() {
        let vault = IdentityVault::new();
        for role in vault.list_roles() {
            let tpl = vault.get(role).unwrap();
            assert!(!tpl.core_competencies.is_empty(), "{:?} 应有核心能力", role);
        }
    }
}
