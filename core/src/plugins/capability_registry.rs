//! 能力注册表
//!
//! 能力插件注册、发现、启用、禁用。

use std::collections::HashMap;

/// 能力状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityStatus {
    Enabled,
    Disabled,
}

/// 能力描述
#[derive(Debug, Clone)]
pub struct CapabilityDescriptor {
    pub name: String,
    pub description: String,
    pub version: String,
    pub status: CapabilityStatus,
}

/// 能力注册表
pub struct CapabilityRegistry {
    capabilities: HashMap<String, CapabilityDescriptor>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            capabilities: HashMap::new(),
        }
    }

    /// 注册能力
    pub fn register(&mut self, name: &str, description: &str, version: &str) -> anyhow::Result<()> {
        if self.capabilities.contains_key(name) {
            anyhow::bail!("能力已注册: {}", name);
        }
        self.capabilities.insert(
            name.to_string(),
            CapabilityDescriptor {
                name: name.to_string(),
                description: description.to_string(),
                version: version.to_string(),
                status: CapabilityStatus::Enabled,
            },
        );
        Ok(())
    }

    /// 启用能力
    pub fn enable(&mut self, name: &str) -> anyhow::Result<()> {
        let cap = self
            .capabilities
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("能力未注册: {}", name))?;
        cap.status = CapabilityStatus::Enabled;
        Ok(())
    }

    /// 禁用能力
    pub fn disable(&mut self, name: &str) -> anyhow::Result<()> {
        let cap = self
            .capabilities
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("能力未注册: {}", name))?;
        cap.status = CapabilityStatus::Disabled;
        Ok(())
    }

    /// 查询能力是否启用
    pub fn is_enabled(&self, name: &str) -> bool {
        self.capabilities
            .get(name)
            .map(|c| c.status == CapabilityStatus::Enabled)
            .unwrap_or(false)
    }

    /// 列出所有能力
    pub fn list(&self) -> Vec<&CapabilityDescriptor> {
        self.capabilities.values().collect()
    }

    /// 列出已启用的能力
    pub fn list_enabled(&self) -> Vec<&CapabilityDescriptor> {
        self.capabilities
            .values()
            .filter(|c| c.status == CapabilityStatus::Enabled)
            .collect()
    }

    /// 已注册能力数
    pub fn count(&self) -> usize {
        self.capabilities.len()
    }

    /// 获取能力描述
    pub fn get(&self, name: &str) -> Option<&CapabilityDescriptor> {
        self.capabilities.get(name)
    }

    /// 注销能力
    pub fn unregister(&mut self, name: &str) -> anyhow::Result<()> {
        self.capabilities
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| anyhow::anyhow!("能力未注册: {}", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_get() {
        let mut reg = CapabilityRegistry::new();
        reg.register("tts", "Text to speech", "1.0.0").unwrap();
        let cap = reg.get("tts").unwrap();
        assert_eq!(cap.name, "tts");
        assert_eq!(cap.description, "Text to speech");
        assert_eq!(cap.version, "1.0.0");
        assert_eq!(cap.status, CapabilityStatus::Enabled);
    }

    #[test]
    fn register_duplicate_fails() {
        let mut reg = CapabilityRegistry::new();
        reg.register("tts", "Text to speech", "1.0.0").unwrap();
        assert!(reg.register("tts", "duplicate", "2.0.0").is_err());
    }

    #[test]
    fn unregister_removes_capability() {
        let mut reg = CapabilityRegistry::new();
        reg.register("tts", "Text to speech", "1.0.0").unwrap();
        reg.unregister("tts").unwrap();
        assert!(reg.get("tts").is_none());
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn unregister_missing_fails() {
        let mut reg = CapabilityRegistry::new();
        assert!(reg.unregister("nope").is_err());
    }

    #[test]
    fn enable_disable_toggle() {
        let mut reg = CapabilityRegistry::new();
        reg.register("tts", "Text to speech", "1.0.0").unwrap();
        assert!(reg.is_enabled("tts"));

        reg.disable("tts").unwrap();
        assert!(!reg.is_enabled("tts"));

        reg.enable("tts").unwrap();
        assert!(reg.is_enabled("tts"));
    }

    #[test]
    fn enable_missing_fails() {
        let mut reg = CapabilityRegistry::new();
        assert!(reg.enable("nope").is_err());
    }

    #[test]
    fn disable_missing_fails() {
        let mut reg = CapabilityRegistry::new();
        assert!(reg.disable("nope").is_err());
    }

    #[test]
    fn list_and_list_enabled() {
        let mut reg = CapabilityRegistry::new();
        reg.register("a", "A", "1.0").unwrap();
        reg.register("b", "B", "1.0").unwrap();
        reg.register("c", "C", "1.0").unwrap();
        reg.disable("b").unwrap();

        assert_eq!(reg.list().len(), 3);
        assert_eq!(reg.list_enabled().len(), 2);
        assert_eq!(reg.count(), 3);
    }

    #[test]
    fn is_enabled_unknown_returns_false() {
        let reg = CapabilityRegistry::new();
        assert!(!reg.is_enabled("nonexistent"));
    }
}
