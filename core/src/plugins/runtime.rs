//! WASM 运行时
//!
//! 封装 wasmi，提供 WASM 模块的加载和执行能力。

use std::path::Path;
use wasmi::{Engine, Linker, Module, Store};

/// WASM 运行时
pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    /// 创建 WASM 运行时
    pub fn new() -> anyhow::Result<Self> {
        let engine = Engine::default();
        Ok(Self { engine })
    }

    /// 从字节码加载 WASM 模块
    pub fn load_module_from_bytes(&self, bytes: &[u8]) -> anyhow::Result<WasmModule> {
        let module = Module::new(&self.engine, bytes)?;
        Ok(WasmModule {
            module,
            engine: self.engine.clone(),
        })
    }

    /// 从文件加载 WASM 模块
    pub fn load_module(&self, path: &Path) -> anyhow::Result<WasmModule> {
        let bytes = std::fs::read(path)?;
        self.load_module_from_bytes(&bytes)
    }

    /// 执行插件函数
    pub fn call(
        &self,
        wasm_module: &WasmModule,
        function: &str,
        args: &[WasmValue],
    ) -> anyhow::Result<WasmValue> {
        wasm_module.call(function, args)
    }
}

/// WASM 模块
pub struct WasmModule {
    module: Module,
    engine: Engine,
}

impl WasmModule {
    /// 调用导出函数
    pub fn call(&self, function: &str, args: &[WasmValue]) -> anyhow::Result<WasmValue> {
        let mut store = Store::new(&self.engine, ());
        let linker = Linker::<()>::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &self.module)?
            .start(&mut store)?;

        let func = instance
            .get_func(&store, function)
            .ok_or_else(|| anyhow::anyhow!("导出函数 '{}' 未找到", function))?;

        let params: Vec<wasmi::Val> = args.iter().map(WasmValue::to_val).collect();
        let func_type = func.ty(&store);
        let result_count = func_type.results().len();

        let mut results = vec![wasmi::Val::I32(0); result_count];
        func.call(&mut store, &params, &mut results)?;

        if results.is_empty() {
            Ok(WasmValue::I32(0))
        } else {
            Ok(WasmValue::from_val(&results[0]))
        }
    }
}

/// WASM 值类型
#[derive(Debug, Clone, PartialEq)]
pub enum WasmValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl WasmValue {
    fn to_val(&self) -> wasmi::Val {
        match self {
            WasmValue::I32(v) => wasmi::Val::I32(*v),
            WasmValue::I64(v) => wasmi::Val::I64(*v),
            WasmValue::F32(v) => wasmi::Val::F32((*v).into()),
            WasmValue::F64(v) => wasmi::Val::F64((*v).into()),
        }
    }

    fn from_val(val: &wasmi::Val) -> Self {
        match val {
            wasmi::Val::I32(v) => WasmValue::I32(*v),
            wasmi::Val::I64(v) => WasmValue::I64(*v),
            wasmi::Val::F32(v) => WasmValue::F32((*v).into()),
            wasmi::Val::F64(v) => WasmValue::F64((*v).into()),
            _ => WasmValue::I32(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // WAT: (module (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add))
    const ADD_WASM: &[u8] = &[
        0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, // magic + version
        0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F, // type section
        0x03, 0x02, 0x01, 0x00, // function section
        0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, // export "add"
        0x0A, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B, // code
    ];

    #[test]
    fn create_runtime() {
        let rt = WasmRuntime::new().unwrap();
        assert!(rt.load_module_from_bytes(ADD_WASM).is_ok());
    }

    #[test]
    fn load_and_call_add() {
        let rt = WasmRuntime::new().unwrap();
        let module = rt.load_module_from_bytes(ADD_WASM).unwrap();
        let result = rt
            .call(&module, "add", &[WasmValue::I32(3), WasmValue::I32(4)])
            .unwrap();
        assert_eq!(result, WasmValue::I32(7));
    }

    #[test]
    fn call_nonexistent_function() {
        let rt = WasmRuntime::new().unwrap();
        let module = rt.load_module_from_bytes(ADD_WASM).unwrap();
        let result = rt.call(&module, "nonexistent", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn load_invalid_bytes() {
        let rt = WasmRuntime::new().unwrap();
        let result = rt.load_module_from_bytes(&[0x00, 0x01, 0x02]);
        assert!(result.is_err());
    }

    #[test]
    fn wasm_value_roundtrip() {
        let vals = vec![
            WasmValue::I32(42),
            WasmValue::I64(100),
            WasmValue::F32(1.5),
            WasmValue::F64(1.618),
        ];
        for v in &vals {
            let wasmi_val = v.to_val();
            let back = WasmValue::from_val(&wasmi_val);
            assert_eq!(*v, back);
        }
    }

    #[test]
    fn call_add_negative_numbers() {
        let rt = WasmRuntime::new().unwrap();
        let module = rt.load_module_from_bytes(ADD_WASM).unwrap();
        let result = rt
            .call(&module, "add", &[WasmValue::I32(-5), WasmValue::I32(3)])
            .unwrap();
        assert_eq!(result, WasmValue::I32(-2));
    }
}
