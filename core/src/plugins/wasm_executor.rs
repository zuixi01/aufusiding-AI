//! WASM 插件执行器
//!
//! 将 WASM 模块封装为 PluginExecutor，支持通过 PluginManager 统一管理。

use super::manager::PluginExecutor;
use super::runtime::{WasmModule, WasmRuntime, WasmValue};
use super::sandbox::Sandbox;

/// WASM 插件执行器
pub struct WasmPluginExecutor {
    name: String,
    version: String,
    runtime: WasmRuntime,
    module: WasmModule,
    sandbox: Sandbox,
}

impl WasmPluginExecutor {
    /// 创建 WASM 插件执行器
    pub fn new(
        name: String,
        version: String,
        wasm_bytes: &[u8],
        sandbox: Sandbox,
    ) -> anyhow::Result<Self> {
        let runtime = WasmRuntime::new()?;
        let module = runtime.load_module_from_bytes(wasm_bytes)?;
        Ok(Self {
            name,
            version,
            runtime,
            module,
            sandbox,
        })
    }

    /// 获取沙箱引用
    pub fn sandbox(&self) -> &Sandbox {
        &self.sandbox
    }
}

impl PluginExecutor for WasmPluginExecutor {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn execute(&self, input: &str) -> Result<String, String> {
        // 解析 JSON 输入: {"function": "name", "args": [1, 2]}
        let parsed: serde_json::Value =
            serde_json::from_str(input).map_err(|e| format!("JSON 解析失败: {e}"))?;

        let function = parsed["function"].as_str().ok_or("缺少 'function' 字段")?;

        let args: Vec<WasmValue> = parsed["args"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| {
                if let Some(i) = v.as_i64() {
                    WasmValue::I32(i as i32)
                } else if let Some(f) = v.as_f64() {
                    WasmValue::F64(f)
                } else {
                    WasmValue::I32(0)
                }
            })
            .collect();

        let result = self
            .runtime
            .call(&self.module, function, &args)
            .map_err(|e| format!("WASM 调用失败: {e}"))?;

        let output = match result {
            WasmValue::I32(v) => serde_json::json!({"result": v}),
            WasmValue::I64(v) => serde_json::json!({"result": v}),
            WasmValue::F32(v) => serde_json::json!({"result": v}),
            WasmValue::F64(v) => serde_json::json!({"result": v}),
        };

        Ok(output.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::sandbox::Permissions;

    // WAT add module bytecode
    const ADD_WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F,
        0x01, 0x7F, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00,
        0x0A, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B,
    ];

    fn default_sandbox() -> Sandbox {
        Sandbox::new(Permissions::default())
    }

    #[test]
    fn create_wasm_executor() {
        let exec = WasmPluginExecutor::new(
            "test-plugin".into(),
            "0.1.0".into(),
            ADD_WASM,
            default_sandbox(),
        );
        assert!(exec.is_ok());
    }

    #[test]
    fn execute_add_via_json() {
        let exec =
            WasmPluginExecutor::new("adder".into(), "1.0.0".into(), ADD_WASM, default_sandbox())
                .unwrap();

        let input = r#"{"function": "add", "args": [10, 20]}"#;
        let result = exec.execute(input).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["result"], 30);
    }

    #[test]
    fn execute_invalid_json() {
        let exec =
            WasmPluginExecutor::new("adder".into(), "1.0.0".into(), ADD_WASM, default_sandbox())
                .unwrap();

        let result = exec.execute("not json");
        assert!(result.is_err());
    }

    #[test]
    fn execute_missing_function_field() {
        let exec =
            WasmPluginExecutor::new("adder".into(), "1.0.0".into(), ADD_WASM, default_sandbox())
                .unwrap();

        let result = exec.execute(r#"{"args": [1, 2]}"#);
        assert!(result.is_err());
    }

    #[test]
    fn execute_nonexistent_wasm_function() {
        let exec =
            WasmPluginExecutor::new("adder".into(), "1.0.0".into(), ADD_WASM, default_sandbox())
                .unwrap();

        let result = exec.execute(r#"{"function": "multiply", "args": [2, 3]}"#);
        assert!(result.is_err());
    }

    #[test]
    fn plugin_executor_trait_methods() {
        let exec = WasmPluginExecutor::new(
            "my-plugin".into(),
            "2.0.0".into(),
            ADD_WASM,
            default_sandbox(),
        )
        .unwrap();

        assert_eq!(exec.name(), "my-plugin");
        assert_eq!(exec.version(), "2.0.0");
    }

    #[test]
    fn invalid_wasm_bytes() {
        let result = WasmPluginExecutor::new(
            "bad".into(),
            "0.0.1".into(),
            &[0xFF, 0xFF],
            default_sandbox(),
        );
        assert!(result.is_err());
    }
}
