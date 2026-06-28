//! 权限体系
//!
//! 细粒度的权限管理，控制 AI 对系统资源的访问。

use serde::{Deserialize, Serialize};

/// 权限类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    /// 读取文件
    ReadFile(String),
    /// 写入文件
    WriteFile(String),
    /// 执行命令
    ExecuteCommand,
    /// 网络访问
    NetworkAccess,
    /// 访问金库
    AccessVault,
    /// 控制 IoT 设备
    ControlDevice(String),
    /// 观察屏幕内容
    ObserveScreen,
    /// 执行屏幕交互计划
    ControlScreen,
}

/// 权限管理器
pub struct PermissionManager {
    /// 已授予的权限
    _granted_permissions: Vec<Permission>,
}

impl PermissionManager {
    /// 创建权限管理器
    pub fn new() -> Self {
        Self {
            _granted_permissions: Vec::new(),
        }
    }

    /// 请求权限
    pub async fn request(&mut self, permission: Permission) -> anyhow::Result<bool> {
        if !self.check(&permission) {
            self._granted_permissions.push(permission);
        }
        Ok(true)
    }

    /// 检查权限
    pub fn check(&self, permission: &Permission) -> bool {
        self._granted_permissions.iter().any(|granted| {
            granted == permission
                || matches!(
                    (granted, permission),
                    (Permission::ControlDevice(device), Permission::ControlDevice(_)) if device == "*"
                )
        })
    }

    /// 撤销权限
    pub fn revoke(&mut self, permission: &Permission) -> anyhow::Result<()> {
        let original_len = self._granted_permissions.len();
        self._granted_permissions
            .retain(|granted| granted != permission);
        if self._granted_permissions.len() == original_len {
            anyhow::bail!("permission not granted");
        }
        Ok(())
    }

    /// 列出所有权限
    pub fn list(&self) -> &[Permission] {
        &self._granted_permissions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn request_grants_permission_and_check_reflects_it() {
        let mut manager = PermissionManager::new();
        let granted = manager
            .request(Permission::ControlDevice("lamp-1".to_string()))
            .await
            .unwrap();

        assert!(granted);
        assert!(manager.check(&Permission::ControlDevice("lamp-1".to_string())));
    }

    #[tokio::test]
    async fn revoke_removes_previously_granted_permission() {
        let mut manager = PermissionManager::new();
        let permission = Permission::ControlDevice("lamp-2".to_string());

        manager.request(permission.clone()).await.unwrap();
        manager.revoke(&permission).unwrap();

        assert!(!manager.check(&permission));
    }

    #[tokio::test]
    async fn wildcard_device_permission_matches_any_device() {
        let mut manager = PermissionManager::new();
        manager
            .request(Permission::ControlDevice("*".to_string()))
            .await
            .unwrap();
        assert!(manager.check(&Permission::ControlDevice("lamp-1".to_string())));
        assert!(manager.check(&Permission::ControlDevice("thermostat".to_string())));
    }

    #[tokio::test]
    async fn specific_device_permission_does_not_match_other() {
        let mut manager = PermissionManager::new();
        manager
            .request(Permission::ControlDevice("lamp-1".to_string()))
            .await
            .unwrap();
        assert!(!manager.check(&Permission::ControlDevice("lamp-2".to_string())));
    }

    #[tokio::test]
    async fn list_returns_all_granted() {
        let mut manager = PermissionManager::new();
        manager.request(Permission::NetworkAccess).await.unwrap();
        manager.request(Permission::ExecuteCommand).await.unwrap();
        assert_eq!(manager.list().len(), 2);
    }

    #[tokio::test]
    async fn revoke_nonexistent_returns_error() {
        let mut manager = PermissionManager::new();
        let err = manager.revoke(&Permission::NetworkAccess).unwrap_err();
        assert!(err.to_string().contains("not granted"));
    }

    #[tokio::test]
    async fn duplicate_request_is_idempotent() {
        let mut manager = PermissionManager::new();
        manager.request(Permission::NetworkAccess).await.unwrap();
        manager.request(Permission::NetworkAccess).await.unwrap();
        // should still be only one entry
        assert_eq!(manager.list().len(), 1);
    }

    #[tokio::test]
    async fn check_after_revoke_returns_false() {
        let mut manager = PermissionManager::new();
        manager.request(Permission::AccessVault).await.unwrap();
        assert!(manager.check(&Permission::AccessVault));
        manager.revoke(&Permission::AccessVault).unwrap();
        assert!(!manager.check(&Permission::AccessVault));
    }
}
