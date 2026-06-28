//! 场景管理
//!
//! 编排多个设备动作，实现一键触发的智能场景。
//! 支持预设场景和自定义场景。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// 场景中的单个设备动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneAction {
    pub device_id: String,
    pub command: String,
    pub params: Value,
    pub delay_ms: u64,
}

/// 智能场景
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub description: String,
    pub actions: Vec<SceneAction>,
}

impl Scene {
    /// 预设场景: 回家模式 — 开灯+空调+音乐
    pub fn come_home() -> Self {
        Self {
            id: "come-home".into(),
            name: "回家模式".into(),
            description: "开灯+空调+音乐".into(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".into(),
                    command: "turn_on".into(),
                    params: serde_json::json!({"brightness": 80}),
                    delay_ms: 0,
                },
                SceneAction {
                    device_id: "thermostat-main".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"temperature": 23}),
                    delay_ms: 500,
                },
                SceneAction {
                    device_id: "speaker-living-room".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"play": "welcome_playlist"}),
                    delay_ms: 1000,
                },
            ],
        }
    }

    /// 预设场景: 离家模式 — 关灯+锁门+布防
    pub fn leave_home() -> Self {
        Self {
            id: "leave-home".into(),
            name: "离家模式".into(),
            description: "关灯+锁门+布防".into(),
            actions: vec![
                SceneAction {
                    device_id: "light-all".into(),
                    command: "turn_off".into(),
                    params: Value::Null,
                    delay_ms: 0,
                },
                SceneAction {
                    device_id: "lock-front-door".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"locked": true}),
                    delay_ms: 500,
                },
                SceneAction {
                    device_id: "security-system".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"armed": true}),
                    delay_ms: 1000,
                },
            ],
        }
    }

    /// 预设场景: 晚安模式 — 调暗灯光+锁门+降温
    pub fn good_night() -> Self {
        Self {
            id: "good-night".into(),
            name: "晚安模式".into(),
            description: "调暗灯光+锁门+降温".into(),
            actions: vec![
                SceneAction {
                    device_id: "light-all".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"brightness": 10}),
                    delay_ms: 0,
                },
                SceneAction {
                    device_id: "lock-front-door".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"locked": true}),
                    delay_ms: 500,
                },
                SceneAction {
                    device_id: "thermostat-main".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"temperature": 19}),
                    delay_ms: 1000,
                },
            ],
        }
    }

    /// 预设场景: 观影模式 — 调暗灯光+开电视+调音量
    pub fn movie_time() -> Self {
        Self {
            id: "movie-time".into(),
            name: "观影模式".into(),
            description: "调暗灯光+开电视+调音量".into(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"brightness": 5}),
                    delay_ms: 0,
                },
                SceneAction {
                    device_id: "tv-living-room".into(),
                    command: "turn_on".into(),
                    params: Value::Null,
                    delay_ms: 500,
                },
                SceneAction {
                    device_id: "tv-living-room".into(),
                    command: "set_property".into(),
                    params: serde_json::json!({"volume": 40}),
                    delay_ms: 2000,
                },
            ],
        }
    }
}

/// 场景管理器
pub struct SceneManager {
    scenes: HashMap<String, Scene>,
}

impl SceneManager {
    pub fn new() -> Self {
        Self {
            scenes: HashMap::new(),
        }
    }

    /// 添加场景
    pub fn add_scene(&mut self, scene: Scene) {
        self.scenes.insert(scene.id.clone(), scene);
    }

    /// 移除场景
    pub fn remove_scene(&mut self, scene_id: &str) -> Option<Scene> {
        self.scenes.remove(scene_id)
    }

    /// 获取场景
    pub fn get_scene(&self, scene_id: &str) -> Option<&Scene> {
        self.scenes.get(scene_id)
    }

    /// 列出所有场景
    pub fn list_scenes(&self) -> Vec<&Scene> {
        self.scenes.values().collect()
    }

    /// 验证场景（所有动作必须有非空 device_id 和 command）
    pub fn validate_scene(scene: &Scene) -> anyhow::Result<()> {
        if scene.id.is_empty() {
            anyhow::bail!("scene id must not be empty");
        }
        if scene.name.is_empty() {
            anyhow::bail!("scene name must not be empty");
        }
        if scene.actions.is_empty() {
            anyhow::bail!("scene must have at least one action");
        }
        for (i, action) in scene.actions.iter().enumerate() {
            if action.device_id.is_empty() {
                anyhow::bail!("action {} has empty device_id", i);
            }
            if action.command.is_empty() {
                anyhow::bail!("action {} has empty command", i);
            }
        }
        Ok(())
    }

    /// 获取执行计划（按 delay_ms 排序）
    pub fn execution_plan(&self, scene_id: &str) -> anyhow::Result<Vec<&SceneAction>> {
        let scene = self
            .scenes
            .get(scene_id)
            .ok_or_else(|| anyhow::anyhow!("unknown scene: {}", scene_id))?;
        let mut actions: Vec<&SceneAction> = scene.actions.iter().collect();
        actions.sort_by_key(|a| a.delay_ms);
        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> SceneManager {
        let mut manager = SceneManager::new();
        manager.add_scene(Scene::come_home());
        manager.add_scene(Scene::leave_home());
        manager.add_scene(Scene::good_night());
        manager.add_scene(Scene::movie_time());
        manager
    }

    #[test]
    fn add_and_get_scene() {
        let manager = setup();
        let scene = manager.get_scene("come-home");
        assert!(scene.is_some());
        assert_eq!(scene.unwrap().name, "回家模式");
    }

    #[test]
    fn remove_scene() {
        let mut manager = setup();
        let removed = manager.remove_scene("come-home");
        assert!(removed.is_some());
        assert!(manager.get_scene("come-home").is_none());
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut manager = SceneManager::new();
        assert!(manager.remove_scene("nonexistent").is_none());
    }

    #[test]
    fn list_scenes() {
        let manager = setup();
        let scenes = manager.list_scenes();
        assert_eq!(scenes.len(), 4);
    }

    #[test]
    fn validate_scene_valid() {
        let scene = Scene::come_home();
        assert!(SceneManager::validate_scene(&scene).is_ok());
    }

    #[test]
    fn validate_scene_empty_id() {
        let scene = Scene {
            id: "".into(),
            name: "test".into(),
            description: "test".into(),
            actions: vec![SceneAction {
                device_id: "dev-1".into(),
                command: "turn_on".into(),
                params: Value::Null,
                delay_ms: 0,
            }],
        };
        assert!(SceneManager::validate_scene(&scene).is_err());
    }

    #[test]
    fn validate_scene_empty_name() {
        let scene = Scene {
            id: "test".into(),
            name: "".into(),
            description: "test".into(),
            actions: vec![SceneAction {
                device_id: "dev-1".into(),
                command: "turn_on".into(),
                params: Value::Null,
                delay_ms: 0,
            }],
        };
        assert!(SceneManager::validate_scene(&scene).is_err());
    }

    #[test]
    fn validate_scene_no_actions() {
        let scene = Scene {
            id: "test".into(),
            name: "test".into(),
            description: "test".into(),
            actions: vec![],
        };
        assert!(SceneManager::validate_scene(&scene).is_err());
    }

    #[test]
    fn validate_scene_empty_device_id() {
        let scene = Scene {
            id: "test".into(),
            name: "test".into(),
            description: "test".into(),
            actions: vec![SceneAction {
                device_id: "".into(),
                command: "turn_on".into(),
                params: Value::Null,
                delay_ms: 0,
            }],
        };
        assert!(SceneManager::validate_scene(&scene).is_err());
    }

    #[test]
    fn validate_scene_empty_command() {
        let scene = Scene {
            id: "test".into(),
            name: "test".into(),
            description: "test".into(),
            actions: vec![SceneAction {
                device_id: "dev-1".into(),
                command: "".into(),
                params: Value::Null,
                delay_ms: 0,
            }],
        };
        assert!(SceneManager::validate_scene(&scene).is_err());
    }

    #[test]
    fn execution_plan_sorted_by_delay() {
        let manager = setup();
        let plan = manager.execution_plan("come-home").unwrap();
        assert_eq!(plan.len(), 3);
        assert!(plan[0].delay_ms <= plan[1].delay_ms);
        assert!(plan[1].delay_ms <= plan[2].delay_ms);
    }

    #[test]
    fn execution_plan_unknown_scene() {
        let manager = SceneManager::new();
        assert!(manager.execution_plan("nonexistent").is_err());
    }

    #[test]
    fn preset_scenes_are_valid() {
        let presets = vec![
            Scene::come_home(),
            Scene::leave_home(),
            Scene::good_night(),
            Scene::movie_time(),
        ];
        for scene in &presets {
            assert!(
                SceneManager::validate_scene(scene).is_ok(),
                "preset scene '{}' should be valid",
                scene.id
            );
        }
    }

    #[test]
    fn preset_come_home_has_correct_actions() {
        let scene = Scene::come_home();
        assert_eq!(scene.actions.len(), 3);
        assert_eq!(scene.actions[0].device_id, "light-living-room");
    }

    #[test]
    fn preset_leave_home_has_correct_actions() {
        let scene = Scene::leave_home();
        assert_eq!(scene.actions.len(), 3);
        assert_eq!(scene.actions[0].command, "turn_off");
    }

    #[test]
    fn preset_movie_time_has_correct_actions() {
        let scene = Scene::movie_time();
        assert_eq!(scene.actions.len(), 3);
        assert_eq!(scene.actions[1].device_id, "tv-living-room");
    }
}
