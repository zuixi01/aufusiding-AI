//! 沙箱权限管理
//!
//! 定义插件的权限模型，限制插件对系统资源的访问。

use serde::{Deserialize, Serialize};

/// 插件权限
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Permissions {
    /// 允许网络访问
    pub network: bool,
    /// 允许文件系统访问
    pub filesystem: bool,
    /// 允许 WASM 执行
    pub wasm_execute: bool,
    /// 允许的文件路径白名单
    pub allowed_paths: Vec<String>,
    /// 允许的域名白名单
    pub allowed_domains: Vec<String>,
}

/// 沙箱环境
pub struct Sandbox {
    pub permissions: Permissions,
}

impl Sandbox {
    pub fn new(permissions: Permissions) -> Self {
        Self { permissions }
    }

    /// 检查是否允许网络访问指定域名
    pub fn check_network(&self, domain: &str) -> bool {
        if !self.permissions.network {
            return false;
        }
        if self.permissions.allowed_domains.is_empty() {
            return true; // 空白名单 = 允许所有
        }
        self.permissions
            .allowed_domains
            .iter()
            .any(|d| d == domain || domain.ends_with(&format!(".{d}")))
    }

    /// 检查是否允许文件访问指定路径
    pub fn check_filesystem(&self, path: &str) -> bool {
        if !self.permissions.filesystem {
            return false;
        }
        if self.permissions.allowed_paths.is_empty() {
            return true;
        }
        self.permissions
            .allowed_paths
            .iter()
            .any(|p| path.starts_with(p))
    }

    /// 检查是否允许 WASM 执行
    pub fn check_wasm_execute(&self) -> bool {
        self.permissions.wasm_execute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_network_permission() {
        let sb = Sandbox::new(Permissions::default());
        assert!(!sb.check_network("example.com"));
    }

    #[test]
    fn network_all_domains() {
        let sb = Sandbox::new(Permissions {
            network: true,
            allowed_domains: vec![],
            ..Default::default()
        });
        assert!(sb.check_network("anything.com"));
    }

    #[test]
    fn network_whitelist() {
        let sb = Sandbox::new(Permissions {
            network: true,
            allowed_domains: vec!["api.weather.com".into()],
            ..Default::default()
        });
        assert!(sb.check_network("api.weather.com"));
        assert!(!sb.check_network("evil.com"));
    }

    #[test]
    fn network_subdomain_match() {
        let sb = Sandbox::new(Permissions {
            network: true,
            allowed_domains: vec!["weather.com".into()],
            ..Default::default()
        });
        assert!(sb.check_network("api.weather.com"));
        assert!(!sb.check_network("notweather.com"));
    }

    #[test]
    fn no_filesystem_permission() {
        let sb = Sandbox::new(Permissions::default());
        let path = crate::test_support::temp_path("sandbox-data");
        assert!(!sb.check_filesystem(path.to_string_lossy().as_ref()));
    }

    #[test]
    fn filesystem_whitelist() {
        let plugin_dir = crate::test_support::temp_dir_path("plugins");
        let allowed_path = plugin_dir.to_string_lossy().to_string();
        let plugin_file = plugin_dir.join("data.json");
        let sb = Sandbox::new(Permissions {
            filesystem: true,
            allowed_paths: vec![allowed_path],
            ..Default::default()
        });
        assert!(sb.check_filesystem(plugin_file.to_string_lossy().as_ref()));
        assert!(!sb.check_filesystem("C:\\Windows\\System32\\drivers\\etc\\hosts"));
    }

    #[test]
    fn filesystem_all_paths() {
        let sb = Sandbox::new(Permissions {
            filesystem: true,
            allowed_paths: vec![],
            ..Default::default()
        });
        assert!(sb.check_filesystem("/any/path"));
    }

    #[test]
    fn wasm_execute_denied_by_default() {
        let sb = Sandbox::new(Permissions::default());
        assert!(!sb.check_wasm_execute());
    }

    #[test]
    fn wasm_execute_allowed() {
        let sb = Sandbox::new(Permissions {
            wasm_execute: true,
            ..Default::default()
        });
        assert!(sb.check_wasm_execute());
    }
}
