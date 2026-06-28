//! 适配器工厂
//!
//! 从 AdapterConfig 自动实例化设备适配器并注册到 GenericController。

use std::sync::Arc;

use crate::config::adapter_config::AdapterConfig;
use crate::iot::adapters::homeassistant::HomeAssistantAdapter;
use crate::iot::adapters::mqtt::{MqttAdapter, TokioMqttTransport};
use crate::iot::adapters::tuya::TuyaAdapter;
use crate::iot::control::GenericController;

/// 从配置构建 GenericController，自动注册所有已配置的适配器
pub struct AdapterFactory;

impl AdapterFactory {
    /// 从 AdapterConfig 构建 GenericController
    ///
    /// 遍历所有 IoT 适配器配置项，为每个非空配置创建对应适配器并注册。
    pub fn build(config: &AdapterConfig) -> GenericController {
        let mut controller = GenericController::new();

        if let Some(ref ha_config) = config.homeassistant {
            let adapter = HomeAssistantAdapter::new(ha_config.clone());
            controller.register_adapter(Box::new(adapter));
        }

        if let Some(ref tuya_config) = config.tuya {
            let adapter = TuyaAdapter::new(tuya_config.clone());
            controller.register_adapter(Box::new(adapter));
        }

        if let Some(ref mqtt_config) = config.mqtt {
            let transport = Arc::new(TokioMqttTransport::new(mqtt_config.clone()));
            let adapter = MqttAdapter::new(mqtt_config.clone(), transport);
            controller.register_adapter(Box::new(adapter));
        }

        controller
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::adapters::homeassistant::HomeAssistantConfig;
    use crate::iot::adapters::mqtt::MqttConfig;
    use crate::iot::adapters::tuya::{TuyaConfig, TuyaRegion};

    fn ha_config() -> HomeAssistantConfig {
        HomeAssistantConfig {
            host: "127.0.0.1".into(),
            port: 8123,
            access_token: "test-token".into(),
        }
    }

    fn tuya_config() -> TuyaConfig {
        TuyaConfig {
            client_id: "cid".into(),
            client_secret: "secret".into(),
            region: TuyaRegion::CN,
            uid: "uid".into(),
        }
    }

    fn mqtt_config() -> MqttConfig {
        MqttConfig::default()
    }

    #[test]
    fn empty_config_builds_empty_controller() {
        let cfg = AdapterConfig::default();
        let controller = AdapterFactory::build(&cfg);
        // 无适配器，execute 应失败
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(controller.execute(
            "any-device",
            "turn_on",
            serde_json::json!({}),
        ));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn homeassistant_config_registers_adapter() {
        let mut cfg = AdapterConfig::default();
        cfg.homeassistant = Some(ha_config());
        let controller = AdapterFactory::build(&cfg);
        // HA 适配器已注册，但设备不存在时会返回 HTTP 错误（非 "no adapter" 错误）
        let result = controller
            .execute("light.test", "get_state", serde_json::json!({"device_type": "HomeAssistant"}))
            .await;
        // 应该到达 HA 适配器（可能连接失败，但不是 "no adapter" 错误）
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(!err_msg.contains("no adapter can handle"));
    }

    #[tokio::test]
    async fn tuya_config_registers_adapter() {
        let mut cfg = AdapterConfig::default();
        cfg.tuya = Some(tuya_config());
        let controller = AdapterFactory::build(&cfg);
        let result = controller
            .execute("device-1", "get_state", serde_json::json!({"device_type": "SmartHome"}))
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(!err_msg.contains("no adapter can handle"));
    }

    #[tokio::test]
    async fn mqtt_config_registers_adapter() {
        let mut cfg = AdapterConfig::default();
        cfg.mqtt = Some(mqtt_config());
        let controller = AdapterFactory::build(&cfg);
        let result = controller
            .execute("sensor-1", "get_state", serde_json::json!({"device_type": "mqtt"}))
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(!err_msg.contains("no adapter can handle"));
    }

    #[tokio::test]
    async fn multiple_adapters_registered() {
        let mut cfg = AdapterConfig::default();
        cfg.homeassistant = Some(ha_config());
        cfg.tuya = Some(tuya_config());
        cfg.mqtt = Some(mqtt_config());
        let controller = AdapterFactory::build(&cfg);

        // 每种适配器都应注册，不会报 "no adapter"
        for (device_type, device_id) in [
            ("HomeAssistant", "light.test"),
            ("SmartHome", "tuya-dev"),
            ("mqtt", "mqtt-dev"),
        ] {
            let result = controller
                .execute(device_id, "get_state", serde_json::json!({"device_type": device_type}))
                .await;
            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(
                !err_msg.contains("no adapter can handle"),
                "adapter for {} not registered",
                device_type
            );
        }
    }

    #[test]
    fn build_is_deterministic() {
        let mut cfg = AdapterConfig::default();
        cfg.homeassistant = Some(ha_config());
        // 两次构建应该都成功（无副作用）
        let _c1 = AdapterFactory::build(&cfg);
        let _c2 = AdapterFactory::build(&cfg);
    }
}
