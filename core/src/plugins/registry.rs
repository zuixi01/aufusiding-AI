//! 插件注册表
//!
//! 管理已安装插件的元数据、版本和依赖关系。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 插件元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// 插件 ID
    pub id: String,
    /// 插件名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 作者
    pub author: String,
    /// 描述
    pub description: String,
    /// 权限需求
    pub permissions: super::sandbox::Permissions,
}

/// 插件注册表
pub struct PluginRegistry {
    plugins: HashMap<String, PluginMetadata>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, metadata: PluginMetadata) -> anyhow::Result<()> {
        if metadata.id.is_empty() {
            anyhow::bail!("插件 ID 不能为空");
        }
        self.plugins.insert(metadata.id.clone(), metadata);
        Ok(())
    }

    pub fn unregister(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        self.plugins
            .remove(plugin_id)
            .map(|_| ())
            .ok_or_else(|| anyhow::anyhow!("插件 '{plugin_id}' 未注册"))
    }

    pub fn list(&self) -> Vec<&PluginMetadata> {
        self.plugins.values().collect()
    }

    pub fn get(&self, plugin_id: &str) -> Option<&PluginMetadata> {
        self.plugins.get(plugin_id)
    }

    pub fn contains(&self, plugin_id: &str) -> bool {
        self.plugins.contains_key(plugin_id)
    }

    pub fn count(&self) -> usize {
        self.plugins.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::sandbox::Permissions;

    fn test_metadata(id: &str) -> PluginMetadata {
        PluginMetadata {
            id: id.into(),
            name: id.into(),
            version: "0.1.0".into(),
            author: "test".into(),
            description: "test plugin".into(),
            permissions: Permissions::default(),
        }
    }

    #[test]
    fn register_and_get() {
        let mut reg = PluginRegistry::new();
        reg.register(test_metadata("calc")).unwrap();
        assert!(reg.contains("calc"));
        assert_eq!(reg.get("calc").unwrap().name, "calc");
    }

    #[test]
    fn unregister() {
        let mut reg = PluginRegistry::new();
        reg.register(test_metadata("calc")).unwrap();
        reg.unregister("calc").unwrap();
        assert!(!reg.contains("calc"));
    }

    #[test]
    fn unregister_missing_fails() {
        let mut reg = PluginRegistry::new();
        assert!(reg.unregister("nope").is_err());
    }

    #[test]
    fn list_plugins() {
        let mut reg = PluginRegistry::new();
        reg.register(test_metadata("a")).unwrap();
        reg.register(test_metadata("b")).unwrap();
        assert_eq!(reg.list().len(), 2);
        assert_eq!(reg.count(), 2);
    }

    #[test]
    fn empty_id_rejected() {
        let mut reg = PluginRegistry::new();
        assert!(reg.register(test_metadata("")).is_err());
    }
}
