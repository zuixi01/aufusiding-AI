//! 插件管理器
//!
//! 动态加载 cdylib 插件，管理生命周期，执行插件功能。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::registry::{PluginMetadata, PluginRegistry};
use super::sandbox::Permissions;
use super::sandbox::Sandbox;
use super::wasm_executor::WasmPluginExecutor;

/// 已加载的插件实例
struct LoadedPlugin {
    executor: Box<dyn PluginExecutor>,
}

/// 插件执行接口
pub trait PluginExecutor: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn execute(&self, input: &str) -> Result<String, String>;
}

/// 测试用 Mock 插件执行器
pub struct MockPluginExecutor {
    plugin_name: String,
    plugin_version: String,
    response: std::sync::Mutex<String>,
    calls: std::sync::Mutex<Vec<String>>,
}

impl MockPluginExecutor {
    pub fn new(name: &str) -> Self {
        Self {
            plugin_name: name.to_string(),
            plugin_version: "0.1.0".to_string(),
            response: std::sync::Mutex::new(format!("[{}] ok", name)),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn with_response(self, resp: &str) -> Self {
        *self.response.lock().unwrap() = resp.to_string();
        self
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl PluginExecutor for MockPluginExecutor {
    fn name(&self) -> &str {
        &self.plugin_name
    }
    fn version(&self) -> &str {
        &self.plugin_version
    }
    fn execute(&self, input: &str) -> Result<String, String> {
        self.calls.lock().unwrap().push(input.to_string());
        Ok(self.response.lock().unwrap().clone())
    }
}

/// 插件管理器
pub struct PluginManager {
    registry: PluginRegistry,
    plugins: HashMap<String, LoadedPlugin>,
    plugin_dir: PathBuf,
}

impl PluginManager {
    pub fn new(plugin_dir: impl Into<PathBuf>) -> Self {
        Self {
            registry: PluginRegistry::new(),
            plugins: HashMap::new(),
            plugin_dir: plugin_dir.into(),
        }
    }

    /// 注册并加载一个内置插件（非 cdylib，直接传入 executor）
    pub fn register_builtin(
        &mut self,
        id: &str,
        description: &str,
        executor: Box<dyn PluginExecutor>,
    ) -> anyhow::Result<()> {
        let metadata = PluginMetadata {
            id: id.into(),
            name: executor.name().into(),
            version: executor.version().into(),
            author: "builtin".into(),
            description: description.into(),
            permissions: Permissions::default(),
        };
        self.registry.register(metadata)?;
        self.plugins.insert(id.into(), LoadedPlugin { executor });
        Ok(())
    }

    /// 执行指定插件
    pub fn execute(&self, plugin_id: &str, input: &str) -> Result<String, String> {
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| format!("插件 '{plugin_id}' 未加载"))?;
        plugin.executor.execute(input)
    }

    /// 列出所有已加载插件
    pub fn list_plugins(&self) -> Vec<&PluginMetadata> {
        self.registry.list()
    }

    /// 获取插件信息
    pub fn get_plugin(&self, plugin_id: &str) -> Option<&PluginMetadata> {
        self.registry.get(plugin_id)
    }

    /// 卸载插件
    pub fn unload(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        self.plugins
            .remove(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("插件 '{plugin_id}' 未加载"))?;
        self.registry.unregister(plugin_id)?;
        Ok(())
    }

    /// 已加载插件数量
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// 插件目录
    pub fn plugin_dir(&self) -> &Path {
        &self.plugin_dir
    }

    /// 检查插件是否已加载
    pub fn is_loaded(&self, plugin_id: &str) -> bool {
        self.plugins.contains_key(plugin_id)
    }

    /// 加载 WASM 插件
    pub fn load_wasm(
        &mut self,
        id: &str,
        description: &str,
        wasm_bytes: &[u8],
        permissions: Permissions,
    ) -> anyhow::Result<()> {
        if !permissions.wasm_execute {
            anyhow::bail!("WASM 执行权限未授予");
        }

        let sandbox = Sandbox::new(permissions.clone());
        let executor =
            WasmPluginExecutor::new(id.to_string(), "0.1.0".to_string(), wasm_bytes, sandbox)?;

        let metadata = PluginMetadata {
            id: id.into(),
            name: executor.name().into(),
            version: executor.version().into(),
            author: "wasm".into(),
            description: description.into(),
            permissions,
        };

        self.registry.register(metadata)?;
        self.plugins.insert(
            id.into(),
            LoadedPlugin {
                executor: Box::new(executor),
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExecutor {
        name: String,
    }

    impl PluginExecutor for MockExecutor {
        fn name(&self) -> &str {
            &self.name
        }
        fn version(&self) -> &str {
            "0.1.0"
        }
        fn execute(&self, input: &str) -> Result<String, String> {
            Ok(format!("[{}] {}", self.name, input))
        }
    }

    fn mock_executor(name: &str) -> Box<dyn PluginExecutor> {
        Box::new(MockExecutor { name: name.into() })
    }

    #[test]
    fn register_and_execute() {
        let mut mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        mgr.register_builtin("calc", "计算器", mock_executor("calculator"))
            .unwrap();

        let result = mgr.execute("calc", "2+3").unwrap();
        assert_eq!(result, "[calculator] 2+3");
    }

    #[test]
    fn execute_unknown_plugin() {
        let mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        assert!(mgr.execute("nope", "test").is_err());
    }

    #[test]
    fn list_and_count() {
        let mut mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        mgr.register_builtin("a", "Plugin A", mock_executor("a"))
            .unwrap();
        mgr.register_builtin("b", "Plugin B", mock_executor("b"))
            .unwrap();
        assert_eq!(mgr.count(), 2);
        assert_eq!(mgr.list_plugins().len(), 2);
    }

    #[test]
    fn unload_plugin() {
        let mut mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        mgr.register_builtin("calc", "计算器", mock_executor("calc"))
            .unwrap();
        assert!(mgr.is_loaded("calc"));

        mgr.unload("calc").unwrap();
        assert!(!mgr.is_loaded("calc"));
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn unload_missing_fails() {
        let mut mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        assert!(mgr.unload("nope").is_err());
    }

    // WAT add module bytecode
    const ADD_WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F,
        0x01, 0x7F, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00,
        0x0A, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B,
    ];

    #[test]
    fn load_wasm_plugin_and_execute() {
        let mut mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        let perms = Permissions {
            wasm_execute: true,
            ..Default::default()
        };
        mgr.load_wasm("wasm-add", "WASM 加法插件", ADD_WASM, perms)
            .unwrap();
        assert!(mgr.is_loaded("wasm-add"));
        assert_eq!(mgr.count(), 1);

        let result = mgr
            .execute("wasm-add", r#"{"function":"add","args":[5,7]}"#)
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["result"], 12);
    }

    #[test]
    fn load_wasm_without_permission_fails() {
        let mut mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        let perms = Permissions::default(); // wasm_execute = false
        let result = mgr.load_wasm("wasm-add", "desc", ADD_WASM, perms);
        assert!(result.is_err());
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn load_wasm_invalid_bytes_fails() {
        let mut mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        let perms = Permissions {
            wasm_execute: true,
            ..Default::default()
        };
        let result = mgr.load_wasm("bad", "desc", &[0xFF], perms);
        assert!(result.is_err());
    }

    #[test]
    fn get_plugin_metadata() {
        let mut mgr = PluginManager::new(crate::test_support::temp_dir_path("plugins"));
        mgr.register_builtin("calc", "计算器插件", mock_executor("calculator"))
            .unwrap();

        let meta = mgr.get_plugin("calc").unwrap();
        assert_eq!(meta.name, "calculator");
        assert_eq!(meta.description, "计算器插件");
    }
}
