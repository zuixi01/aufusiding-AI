//! 设备控制
//!
//! 统一的设备控制接口，支持多种智能家居协议。

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 设备状态
#[derive(Debug, Clone)]
pub struct DeviceState {
    pub device_id: String,
    pub device_type: String,
    pub powered: bool,
    pub properties: HashMap<String, Value>,
    pub last_updated: i64,
}

/// 设备控制器 trait
#[async_trait]
pub trait DeviceController: Send + Sync {
    /// 打开设备
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()>;
    /// 关闭设备
    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()>;
    /// 设置设备属性（亮度、温度等）
    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()>;
    /// 获取设备状态
    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value>;
    /// 支持的设备类型
    fn supported_type(&self) -> &str;
}

/// Mock 设备控制器（内存状态）
pub struct MockDeviceController {
    states: Arc<Mutex<HashMap<String, DeviceState>>>,
    device_type: String,
}

impl MockDeviceController {
    pub fn new(device_type: &str) -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
            device_type: device_type.to_string(),
        }
    }

    pub fn register_device(&self, device_id: &str, device_type: &str) {
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states.insert(
            device_id.to_string(),
            DeviceState {
                device_id: device_id.to_string(),
                device_type: device_type.to_string(),
                powered: false,
                properties: HashMap::new(),
                last_updated: chrono::Utc::now().timestamp(),
            },
        );
    }
}

#[async_trait]
impl DeviceController for MockDeviceController {
    async fn turn_on(&self, device_id: &str) -> anyhow::Result<()> {
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states
            .get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown device: {}", device_id))?;
        state.powered = true;
        state.last_updated = chrono::Utc::now().timestamp();
        Ok(())
    }

    async fn turn_off(&self, device_id: &str) -> anyhow::Result<()> {
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states
            .get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown device: {}", device_id))?;
        state.powered = false;
        state.last_updated = chrono::Utc::now().timestamp();
        Ok(())
    }

    async fn set_property(
        &self,
        device_id: &str,
        property: &str,
        value: Value,
    ) -> anyhow::Result<()> {
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states
            .get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown device: {}", device_id))?;
        state.properties.insert(property.to_string(), value);
        state.last_updated = chrono::Utc::now().timestamp();
        Ok(())
    }

    async fn get_state(&self, device_id: &str) -> anyhow::Result<Value> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states
            .get(device_id)
            .ok_or_else(|| anyhow::anyhow!("unknown device: {}", device_id))?;
        Ok(serde_json::json!({
            "device_id": state.device_id,
            "device_type": state.device_type,
            "powered": state.powered,
            "properties": state.properties,
            "last_updated": state.last_updated,
        }))
    }

    fn supported_type(&self) -> &str {
        &self.device_type
    }
}

/// 通用设备控制器（路由到适配器）
pub struct GenericController {
    adapters: HashMap<String, Box<dyn DeviceController>>,
}

impl GenericController {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// 注册协议适配器（按 supported_type 索引）
    pub fn register_adapter(&mut self, adapter: Box<dyn DeviceController>) {
        let device_type = adapter.supported_type().to_string();
        self.adapters.insert(device_type, adapter);
    }

    /// 根据设备类型查找适配器
    fn find_adapter(&self, device_type: &str) -> Option<&dyn DeviceController> {
        self.adapters.get(device_type).map(|b| b.as_ref())
    }

    /// 执行设备命令（路由到匹配的适配器）
    pub async fn execute(
        &self,
        device_id: &str,
        command: &str,
        params: Value,
    ) -> anyhow::Result<Value> {
        // 先尝试按 device_type 参数路由
        if let Some(device_type) = params.get("device_type").and_then(|v| v.as_str()) {
            if let Some(adapter) = self.find_adapter(device_type) {
                return self
                    .dispatch(adapter, device_id, command, &params)
                    .await;
            }
        }

        // 回退：遍历所有适配器尝试执行
        for adapter in self.adapters.values() {
            match self.dispatch(adapter.as_ref(), device_id, command, &params).await {
                Ok(result) => return Ok(result),
                Err(_) => continue,
            }
        }
        anyhow::bail!("no adapter can handle device: {}", device_id)
    }

    /// 分发命令到具体适配器
    async fn dispatch(
        &self,
        adapter: &dyn DeviceController,
        device_id: &str,
        command: &str,
        params: &Value,
    ) -> anyhow::Result<Value> {
        match command {
            "turn_on" => {
                adapter.turn_on(device_id).await?;
                Ok(serde_json::json!({"status": "ok"}))
            }
            "turn_off" => {
                adapter.turn_off(device_id).await?;
                Ok(serde_json::json!({"status": "ok"}))
            }
            "set_property" => {
                let prop = params
                    .get("property")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let val = params.get("value").cloned().unwrap_or(Value::Null);
                adapter.set_property(device_id, prop, val).await?;
                Ok(serde_json::json!({"status": "ok"}))
            }
            "get_state" => {
                let state = adapter.get_state(device_id).await?;
                Ok(state)
            }
            _ => anyhow::bail!("unknown command: {}", command),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_mock() -> MockDeviceController {
        let ctrl = MockDeviceController::new("Light");
        ctrl.register_device("light-001", "Light");
        ctrl
    }

    #[tokio::test]
    async fn turn_on_off() {
        let ctrl = setup_mock();
        ctrl.turn_on("light-001").await.unwrap();
        let state = ctrl.get_state("light-001").await.unwrap();
        assert_eq!(state["powered"], true);
        ctrl.turn_off("light-001").await.unwrap();
        let state = ctrl.get_state("light-001").await.unwrap();
        assert_eq!(state["powered"], false);
    }

    #[tokio::test]
    async fn set_property_works() {
        let ctrl = setup_mock();
        ctrl.set_property("light-001", "brightness", serde_json::json!(80))
            .await
            .unwrap();
        let state = ctrl.get_state("light-001").await.unwrap();
        assert_eq!(state["properties"]["brightness"], 80);
    }

    #[tokio::test]
    async fn get_state_returns_info() {
        let ctrl = setup_mock();
        let state = ctrl.get_state("light-001").await.unwrap();
        assert_eq!(state["device_id"], "light-001");
        assert_eq!(state["device_type"], "Light");
    }

    #[tokio::test]
    async fn unknown_device_errors() {
        let ctrl = MockDeviceController::new("Light");
        let result = ctrl.turn_on("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generic_register_and_route() {
        let mut gc = GenericController::new();
        let mock = MockDeviceController::new("Light");
        mock.register_device("light-001", "Light");
        gc.register_adapter(Box::new(mock));
        let result = gc
            .execute("light-001", "turn_on", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
    }

    #[tokio::test]
    async fn generic_execute_routes_get_state() {
        let mut gc = GenericController::new();
        let mock = MockDeviceController::new("Light");
        mock.register_device("light-001", "Light");
        gc.register_adapter(Box::new(mock));
        let state = gc
            .execute("light-001", "get_state", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(state["device_id"], "light-001");
    }

    #[tokio::test]
    async fn generic_no_adapter_errors() {
        let gc = GenericController::new();
        let result = gc
            .execute("light-001", "turn_on", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn concurrent_updates() {
        let ctrl = setup_mock();
        ctrl.turn_on("light-001").await.unwrap();
        ctrl.set_property("light-001", "color", serde_json::json!("red"))
            .await
            .unwrap();
        let state = ctrl.get_state("light-001").await.unwrap();
        assert_eq!(state["powered"], true);
        assert_eq!(state["properties"]["color"], "red");
    }

    #[tokio::test]
    async fn generic_routes_by_device_type_param() {
        let mut gc = GenericController::new();

        let light = MockDeviceController::new("Light");
        light.register_device("light-001", "Light");
        gc.register_adapter(Box::new(light));

        let thermostat = MockDeviceController::new("Thermostat");
        thermostat.register_device("thermo-001", "Thermostat");
        gc.register_adapter(Box::new(thermostat));

        // Route to Light adapter via device_type param
        let result = gc
            .execute(
                "light-001",
                "turn_on",
                serde_json::json!({"device_type": "Light"}),
            )
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");

        // Route to Thermostat adapter via device_type param
        let state = gc
            .execute(
                "thermo-001",
                "get_state",
                serde_json::json!({"device_type": "Thermostat"}),
            )
            .await
            .unwrap();
        assert_eq!(state["device_type"], "Thermostat");
    }

    #[tokio::test]
    async fn generic_routes_set_property() {
        let mut gc = GenericController::new();
        let light = MockDeviceController::new("Light");
        light.register_device("light-001", "Light");
        gc.register_adapter(Box::new(light));

        let result = gc
            .execute(
                "light-001",
                "set_property",
                serde_json::json!({"property": "brightness", "value": 75}),
            )
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
    }

    #[tokio::test]
    async fn generic_unknown_command_errors() {
        let mut gc = GenericController::new();
        let light = MockDeviceController::new("Light");
        light.register_device("light-001", "Light");
        gc.register_adapter(Box::new(light));

        let result = gc
            .execute("light-001", "explode", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generic_multiple_adapters_fallback() {
        let mut gc = GenericController::new();

        let light = MockDeviceController::new("Light");
        light.register_device("light-001", "Light");
        gc.register_adapter(Box::new(light));

        let thermostat = MockDeviceController::new("Thermostat");
        thermostat.register_device("thermo-001", "Thermostat");
        gc.register_adapter(Box::new(thermostat));

        // Without device_type param, fallback iterates adapters
        let result = gc
            .execute("light-001", "turn_on", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
    }
}
