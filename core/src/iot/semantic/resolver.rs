//! 设备解析器 — 将 DeviceReference 映射为具体 device_id
//!
//! 支持按名称、房间+类型、ID、环境上下文四种方式解析设备引用，
//! 返回带置信度的解析结果。

use anyhow::{anyhow, Result};

use crate::iot::types::DeviceType;
use super::intent_parser::{ActionType, DeviceReference};
use super::profile::{DeviceSemanticProfile, SemanticProfileStore};

/// 解析后的设备
#[derive(Debug, Clone)]
pub struct ResolvedDevice {
    pub device_id: String,
    pub profile: DeviceSemanticProfile,
    pub confidence: f64,
}

/// 设备解析器
pub struct DeviceResolver<'a> {
    store: &'a SemanticProfileStore,
}

impl<'a> DeviceResolver<'a> {
    pub fn new(store: &'a SemanticProfileStore) -> Self {
        Self { store }
    }

    /// 解析设备引用为具体设备
    pub fn resolve(
        &self,
        reference: &DeviceReference,
        current_room: Option<&str>,
        action: Option<&ActionType>,
    ) -> Result<ResolvedDevice> {
        match reference {
            DeviceReference::ByName(name) => self.resolve_by_name(name),
            DeviceReference::ByRoomAndType(room, device_type) => {
                self.resolve_by_room_and_type(room, device_type)
            }
            DeviceReference::ById(id) => self.resolve_by_id(id),
            DeviceReference::Ambient => self.resolve_ambient(current_room, action),
        }
    }
    /// 按名称解析
    fn resolve_by_name(&self, name: &str) -> Result<ResolvedDevice> {
        let matches = self.store.find_by_name(name);
        match matches.first() {
            Some(m) => Ok(ResolvedDevice {
                device_id: m.0.device_id.clone(),
                profile: m.0.clone(),
                confidence: m.1,
            }),
            None => Err(anyhow!("找不到设备: {}", name)),
        }
    }

    /// 按房间+类型解析
    fn resolve_by_room_and_type(
        &self,
        room: &str,
        device_type: &DeviceType,
    ) -> Result<ResolvedDevice> {
        let room_devices = self.store.query_by_room(room);
        let matched = room_devices
            .into_iter()
            .find(|p| p.device_type == *device_type);
        match matched {
            Some(profile) => Ok(ResolvedDevice {
                device_id: profile.device_id.clone(),
                profile: profile.clone(),
                confidence: 1.0,
            }),
            None => Err(anyhow!("在{}找不到{}设备", room, device_type)),
        }
    }

    /// 按 ID 解析
    fn resolve_by_id(&self, id: &str) -> Result<ResolvedDevice> {
        match self.store.get(id) {
            Some(profile) => Ok(ResolvedDevice {
                device_id: profile.device_id.clone(),
                profile: profile.clone(),
                confidence: 1.0,
            }),
            None => Err(anyhow!("设备ID不存在: {}", id)),
        }
    }
    /// 解析 Ambient 引用 — 从动作类型推断设备类型，在当前房间查找
    fn resolve_ambient(
        &self,
        current_room: Option<&str>,
        action: Option<&ActionType>,
    ) -> Result<ResolvedDevice> {
        let device_type = action.and_then(infer_device_type);

        match (device_type, current_room) {
            (Some(dt), Some(room)) => {
                let room_devices = self.store.query_by_room(room);
                let matched = room_devices
                    .into_iter()
                    .find(|p| p.device_type == dt);
                match matched {
                    Some(profile) => Ok(ResolvedDevice {
                        device_id: profile.device_id.clone(),
                        profile: profile.clone(),
                        confidence: 0.7,
                    }),
                    None => Err(anyhow!("在{}找不到{}设备", room, dt)),
                }
            }
            (Some(dt), None) => {
                let matched = self
                    .store
                    .list_all()
                    .into_iter()
                    .find(|p| p.device_type == dt);
                match matched {
                    Some(profile) => Ok(ResolvedDevice {
                        device_id: profile.device_id.clone(),
                        profile: profile.clone(),
                        confidence: 0.5,
                    }),
                    None => Err(anyhow!("找不到{}设备", dt)),
                }
            }
            _ => Err(anyhow!("请指定要控制的设备")),
        }
    }
}

/// 从动作类型推断设备类型
fn infer_device_type(action: &ActionType) -> Option<DeviceType> {
    match action {
        ActionType::Increase(prop)
        | ActionType::Decrease(prop)
        | ActionType::SetProperty(prop) => match prop.as_str() {
            "brightness" | "color" => Some(DeviceType::Light),
            "temperature" => Some(DeviceType::Thermostat),
            "volume" => Some(DeviceType::Speaker),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::profile::{build_default_profile, SemanticProfileStore};
    use super::super::intent_parser::{ActionType, DeviceReference};
    use crate::iot::types::DeviceType;

    fn build_test_store() -> SemanticProfileStore {
        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile(
            "light-001", "客厅灯", DeviceType::Light, Some("客厅"),
        ));
        store.add(build_default_profile(
            "light-002", "卧室灯", DeviceType::Light, Some("卧室"),
        ));
        store.add(build_default_profile(
            "thermo-001", "客厅空调", DeviceType::Thermostat, Some("客厅"),
        ));
        store.add(build_default_profile(
            "lock-001", "前门锁", DeviceType::Lock, None,
        ));
        store.add(build_default_profile(
            "speaker-001", "客厅音箱", DeviceType::Speaker, Some("客厅"),
        ));
        store
    }

    #[test]
    fn resolve_by_name_exact() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver
            .resolve(&DeviceReference::ByName("客厅灯".into()), None, None)
            .unwrap();
        assert_eq!(result.device_id, "light-001");
        assert!(result.confidence >= 0.9);
    }
    #[test]
    fn resolve_by_name_partial() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver
            .resolve(&DeviceReference::ByName("客厅".into()), None, None)
            .unwrap();
        // Should match one of the 客厅 devices (客厅灯, 客厅空调, 客厅音箱)
        assert!(
            result.device_id == "light-001"
                || result.device_id == "thermo-001"
                || result.device_id == "speaker-001"
        );
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn resolve_by_name_not_found() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver.resolve(
            &DeviceReference::ByName("书房灯".into()),
            None,
            None,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("找不到设备"));
    }

    #[test]
    fn resolve_by_room_and_type() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver
            .resolve(
                &DeviceReference::ByRoomAndType("客厅".into(), DeviceType::Light),
                None,
                None,
            )
            .unwrap();
        assert_eq!(result.device_id, "light-001");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn resolve_by_room_and_type_not_found() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver.resolve(
            &DeviceReference::ByRoomAndType("书房".into(), DeviceType::Light),
            None,
            None,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("书房"));
    }
    #[test]
    fn resolve_by_id_found() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver
            .resolve(&DeviceReference::ById("light-001".into()), None, None)
            .unwrap();
        assert_eq!(result.device_id, "light-001");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn resolve_by_id_not_found() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver.resolve(
            &DeviceReference::ById("nonexistent".into()),
            None,
            None,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("设备ID不存在"));
    }

    #[test]
    fn resolve_ambient_brightness_in_room() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let action = ActionType::Increase("brightness".into());
        let result = resolver
            .resolve(&DeviceReference::Ambient, Some("客厅"), Some(&action))
            .unwrap();
        assert_eq!(result.device_id, "light-001");
        assert_eq!(result.confidence, 0.7);
    }

    #[test]
    fn resolve_ambient_temperature_in_room() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let action = ActionType::Increase("temperature".into());
        let result = resolver
            .resolve(&DeviceReference::Ambient, Some("客厅"), Some(&action))
            .unwrap();
        assert_eq!(result.device_id, "thermo-001");
        assert_eq!(result.confidence, 0.7);
    }

    #[test]
    fn resolve_ambient_volume_in_room() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let action = ActionType::Increase("volume".into());
        let result = resolver
            .resolve(&DeviceReference::Ambient, Some("客厅"), Some(&action))
            .unwrap();
        assert_eq!(result.device_id, "speaker-001");
        assert_eq!(result.confidence, 0.7);
    }
    #[test]
    fn resolve_ambient_no_room() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let action = ActionType::Increase("brightness".into());
        let result = resolver
            .resolve(&DeviceReference::Ambient, None, Some(&action))
            .unwrap();
        // Should find some light device with lower confidence
        assert_eq!(result.profile.device_type, DeviceType::Light);
        assert_eq!(result.confidence, 0.5);
    }

    #[test]
    fn resolve_ambient_no_action() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver.resolve(
            &DeviceReference::Ambient,
            Some("客厅"),
            None,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("请指定要控制的设备"));
    }

    #[test]
    fn resolve_ambient_no_context() {
        let store = build_test_store();
        let resolver = DeviceResolver::new(&store);
        let result = resolver.resolve(&DeviceReference::Ambient, None, None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("请指定要控制的设备"));
    }

    #[test]
    fn infer_device_type_brightness() {
        let action = ActionType::Increase("brightness".into());
        assert_eq!(infer_device_type(&action), Some(DeviceType::Light));
    }

    #[test]
    fn infer_device_type_temperature() {
        let action = ActionType::Decrease("temperature".into());
        assert_eq!(infer_device_type(&action), Some(DeviceType::Thermostat));
    }
}
