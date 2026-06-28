//! 插件系统集成测试
//!
//! 验证 PluginManager + 内置插件 + 注册表 + 沙箱的完整协作。

use prometheus_core::plugins::manager::{PluginExecutor, PluginManager};
use prometheus_core::plugins::sandbox::{Permissions, Sandbox};

// ── 真实插件适配器 ──

struct CalculatorExecutor;

impl PluginExecutor for CalculatorExecutor {
    fn name(&self) -> &str {
        "calculator"
    }
    fn version(&self) -> &str {
        "0.1.0"
    }
    fn execute(&self, input: &str) -> Result<String, String> {
        // 简化版：解析 "a op b" 格式
        let input = input.trim();
        if let Some((a, rest)) = input.split_once('+') {
            let a: f64 = a
                .trim()
                .parse()
                .map_err(|e: std::num::ParseFloatError| e.to_string())?;
            let b: f64 = rest
                .trim()
                .parse()
                .map_err(|e: std::num::ParseFloatError| e.to_string())?;
            return Ok(format!("{}", a + b));
        }
        if let Some((a, rest)) = input.split_once('*') {
            let a: f64 = a
                .trim()
                .parse()
                .map_err(|e: std::num::ParseFloatError| e.to_string())?;
            let b: f64 = rest
                .trim()
                .parse()
                .map_err(|e: std::num::ParseFloatError| e.to_string())?;
            return Ok(format!("{}", a * b));
        }
        Err("不支持的表达式".into())
    }
}

struct WeatherExecutor;

impl PluginExecutor for WeatherExecutor {
    fn name(&self) -> &str {
        "weather"
    }
    fn version(&self) -> &str {
        "0.1.0"
    }
    fn execute(&self, input: &str) -> Result<String, String> {
        if input.trim().is_empty() {
            return Err("城市名不能为空".into());
        }
        Ok(format!("{}：晴，25°C", input.trim()))
    }
}

#[test]
fn plugin_manager_full_lifecycle() {
    let mut mgr = PluginManager::new(prometheus_core::test_support::temp_dir_path("test-plugins"));

    // 注册
    mgr.register_builtin("calculator", "数学计算", Box::new(CalculatorExecutor))
        .unwrap();
    mgr.register_builtin("weather", "天气查询", Box::new(WeatherExecutor))
        .unwrap();
    assert_eq!(mgr.count(), 2);

    // 执行
    let result = mgr.execute("calculator", "3 + 7").unwrap();
    assert_eq!(result, "10");

    let result = mgr.execute("weather", "北京").unwrap();
    assert!(result.contains("北京"));

    // 查询元数据
    let meta = mgr.get_plugin("calculator").unwrap();
    assert_eq!(meta.version, "0.1.0");

    // 卸载
    mgr.unload("weather").unwrap();
    assert_eq!(mgr.count(), 1);
    assert!(mgr.execute("weather", "北京").is_err());

    // calculator 仍可用
    assert_eq!(mgr.execute("calculator", "2 * 5").unwrap(), "10");
}

#[test]
fn sandbox_permission_enforcement() {
    // 无权限沙箱
    let restricted = Sandbox::new(Permissions::default());
    assert!(!restricted.check_network("api.example.com"));
    let denied_path = prometheus_core::test_support::temp_path("sandbox-data");
    assert!(!restricted.check_filesystem(denied_path.to_string_lossy().as_ref()));

    // 受限沙箱
    let plugin_dir = prometheus_core::test_support::temp_dir_path("plugins");
    let plugin_dir_str = plugin_dir.to_string_lossy().to_string();
    let plugin_data = plugin_dir.join("data.json");
    let limited = Sandbox::new(Permissions {
        network: true,
        filesystem: true,
        allowed_domains: vec!["api.weather.com".into()],
        allowed_paths: vec![plugin_dir_str],
        ..Default::default()
    });
    assert!(limited.check_network("api.weather.com"));
    assert!(!limited.check_network("evil.com"));
    assert!(limited.check_filesystem(plugin_data.to_string_lossy().as_ref()));
    assert!(!limited.check_filesystem("C:\\Windows\\System32\\config\\SAM"));
}

#[test]
fn plugin_error_propagation() {
    let mut mgr = PluginManager::new(prometheus_core::test_support::temp_dir_path("test-plugins"));
    mgr.register_builtin("weather", "天气", Box::new(WeatherExecutor))
        .unwrap();

    // 空输入应返回错误
    let err = mgr.execute("weather", "").unwrap_err();
    assert!(err.contains("空"));
}

#[test]
fn duplicate_plugin_id_overwrites() {
    let mut mgr = PluginManager::new(prometheus_core::test_support::temp_dir_path("test-plugins"));
    mgr.register_builtin("calc", "v1", Box::new(CalculatorExecutor))
        .unwrap();
    mgr.register_builtin("calc", "v2", Box::new(WeatherExecutor))
        .unwrap();

    // 第二次注册覆盖了第一次
    let result = mgr.execute("calc", "北京").unwrap();
    assert!(result.contains("北京"));
}
