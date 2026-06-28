use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use super::control::DeviceController;
use super::event_bus::{DeviceEvent, EventBus, EventKind};
use super::scenes::{Scene, SceneAction};
use super::types::DeviceType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionFailure {
    pub device_id: String,
    pub command: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneExecutionResult {
    pub scene_id: String,
    pub total_actions: usize,
    pub succeeded: usize,
    pub failed: Vec<ActionFailure>,
    pub duration_ms: u64,
}

pub struct SceneExecutor {
    controllers: HashMap<String, Arc<dyn DeviceController>>,
    event_bus: Option<Arc<EventBus>>,
}

impl SceneExecutor {
    pub fn new() -> Self {
        Self {
            controllers: HashMap::new(),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn register_controller(&mut self, device_type: &str, controller: Arc<dyn DeviceController>) {
        self.controllers.insert(device_type.to_string(), controller);
    }

    pub async fn execute(&self, scene: &Scene) -> SceneExecutionResult {
        let start = Instant::now();
        let mut actions = scene.actions.clone();
        actions.sort_by_key(|a| a.delay_ms);

        let mut succeeded = 0;
        let mut failed = Vec::new();
        let mut last_delay = 0u64;

        for action in &actions {
            let delta = action.delay_ms.saturating_sub(last_delay);
            if delta > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(delta)).await;
            }
            last_delay = action.delay_ms;

            match self.execute_action(action).await {
                Ok(_) => succeeded += 1,
                Err(e) => {
                    failed.push(ActionFailure {
                        device_id: action.device_id.clone(),
                        command: action.command.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let result = SceneExecutionResult {
            scene_id: scene.id.clone(),
            total_actions: actions.len(),
            succeeded,
            failed,
            duration_ms,
        };

        if let Some(bus) = &self.event_bus {
            let payload = serde_json::json!({
                "scene_id": result.scene_id,
                "succeeded": result.succeeded,
                "failed": result.failed.len(),
            });

            let event = DeviceEvent {
                device_id: format!("scene:{}", scene.id),
                device_type: DeviceType::Other("scene".to_string()),
                event_kind: EventKind::StateChanged,
                payload,
                timestamp: chrono::Utc::now(),
            };

            bus.publish(event);
        }

        result
    }

    pub async fn execute_action(&self, action: &SceneAction) -> anyhow::Result<()> {
        for controller in self.controllers.values() {
            let result = match action.command.as_str() {
                "turn_on" => controller.turn_on(&action.device_id).await,
                "turn_off" => controller.turn_off(&action.device_id).await,
                "set_property" => {
                    // Support two param formats:
                    // 1. {"property": "x", "value": y}
                    // 2. {"x": y} (shorthand used by preset scenes)
                    let (property, value) = if let Some(prop) = action.params.get("property").and_then(|v| v.as_str()) {
                        let val = action.params.get("value").cloned().unwrap_or(serde_json::Value::Null);
                        (prop.to_string(), val)
                    } else if let Some(obj) = action.params.as_object() {
                        if let Some((k, v)) = obj.iter().next() {
                            (k.clone(), v.clone())
                        } else {
                            return Err(anyhow::anyhow!("Empty params for set_property"));
                        }
                    } else {
                        return Err(anyhow::anyhow!("Invalid params for set_property"));
                    };
                    controller.set_property(&action.device_id, &property, value).await
                }
                _ => return Err(anyhow::anyhow!("Unknown command: {}", action.command)),
            };

            if result.is_ok() {
                return result;
            }
        }

        Err(anyhow::anyhow!("No controller could handle device: {}", action.device_id))
    }

    pub async fn dry_run(&self, scene: &Scene) -> Vec<String> {
        let mut unreachable = Vec::new();

        for action in &scene.actions {
            let mut reachable = false;

            for controller in self.controllers.values() {
                if controller.get_state(&action.device_id).await.is_ok() {
                    reachable = true;
                    break;
                }
            }

            if !reachable && !unreachable.contains(&action.device_id) {
                unreachable.push(action.device_id.clone());
            }
        }

        unreachable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::control::MockDeviceController;

    fn setup_light_controller() -> Arc<MockDeviceController> {
        let controller = Arc::new(MockDeviceController::new("light"));
        controller.register_device("light-living-room", "light");
        controller.register_device("light-all", "light");
        controller
    }

    fn setup_thermostat_controller() -> Arc<MockDeviceController> {
        let controller = Arc::new(MockDeviceController::new("thermostat"));
        controller.register_device("thermostat-main", "thermostat");
        controller
    }

    #[tokio::test]
    async fn execute_action_turn_on() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let action = SceneAction {
            device_id: "light-living-room".to_string(),
            command: "turn_on".to_string(),
            params: serde_json::json!({}),
            delay_ms: 0,
        };

        let result = executor.execute_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_action_turn_off() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let action = SceneAction {
            device_id: "light-living-room".to_string(),
            command: "turn_off".to_string(),
            params: serde_json::json!({}),
            delay_ms: 0,
        };

        let result = executor.execute_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_action_set_property() {
        let mut executor = SceneExecutor::new();
        let controller = setup_thermostat_controller();
        executor.register_controller("thermostat", controller);

        let action = SceneAction {
            device_id: "thermostat-main".to_string(),
            command: "set_property".to_string(),
            params: serde_json::json!({
                "property": "temperature",
                "value": 22
            }),
            delay_ms: 0,
        };

        let result = executor.execute_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_action_unknown_command() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let action = SceneAction {
            device_id: "light-living-room".to_string(),
            command: "reboot".to_string(),
            params: serde_json::json!({}),
            delay_ms: 0,
        };

        let result = executor.execute_action(&action).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown command"));
    }

    #[tokio::test]
    async fn execute_empty_scene() {
        let executor = SceneExecutor::new();
        let scene = Scene {
            id: "empty".to_string(),
            name: "Empty Scene".to_string(),
            description: "No actions".to_string(),
            actions: vec![],
        };

        let result = executor.execute(&scene).await;
        assert_eq!(result.total_actions, 0);
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed.len(), 0);
    }

    #[tokio::test]
    async fn execute_single_action() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let scene = Scene {
            id: "single".to_string(),
            name: "Single Action".to_string(),
            description: "One action".to_string(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                }
            ],
        };

        let result = executor.execute(&scene).await;
        assert_eq!(result.total_actions, 1);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed.len(), 0);
    }

    #[tokio::test]
    async fn execute_multi_action_ordered() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let scene = Scene {
            id: "multi".to_string(),
            name: "Multi Action".to_string(),
            description: "Multiple actions".to_string(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 100,
                },
                SceneAction {
                    device_id: "light-all".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_off".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 50,
                },
            ],
        };

        let result = executor.execute(&scene).await;
        assert_eq!(result.total_actions, 3);
        assert_eq!(result.succeeded, 3);
        assert_eq!(result.failed.len(), 0);
    }

    #[tokio::test]
    async fn execute_partial_failure() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let scene = Scene {
            id: "partial".to_string(),
            name: "Partial Failure".to_string(),
            description: "Some actions fail".to_string(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
                SceneAction {
                    device_id: "unknown-device".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
            ],
        };

        let result = executor.execute(&scene).await;
        assert_eq!(result.total_actions, 2);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].device_id, "unknown-device");
    }

    #[tokio::test]
    async fn execute_all_fail() {
        let executor = SceneExecutor::new();

        let scene = Scene {
            id: "fail".to_string(),
            name: "All Fail".to_string(),
            description: "All actions fail".to_string(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
            ],
        };

        let result = executor.execute(&scene).await;
        assert_eq!(result.total_actions, 1);
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed.len(), 1);
    }

    #[tokio::test]
    async fn execute_records_duration() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let scene = Scene {
            id: "duration".to_string(),
            name: "Duration Test".to_string(),
            description: "Test duration recording".to_string(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 10,
                },
            ],
        };

        let result = executor.execute(&scene).await;
        assert!(result.duration_ms > 0);
    }

    #[tokio::test]
    async fn execute_publishes_event() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let event_bus = Arc::new(EventBus::new(10));
        let mut receiver = event_bus.subscribe();
        executor = executor.with_event_bus(event_bus);

        let scene = Scene {
            id: "event".to_string(),
            name: "Event Test".to_string(),
            description: "Test event publishing".to_string(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
            ],
        };

        executor.execute(&scene).await;

        let event = receiver.recv().await.unwrap();
        assert_eq!(event.device_id, "scene:event");
        assert_eq!(event.device_type, DeviceType::Other("scene".to_string()));
    }

    #[tokio::test]
    async fn dry_run_all_reachable() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let scene = Scene {
            id: "reachable".to_string(),
            name: "All Reachable".to_string(),
            description: "All devices reachable".to_string(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
            ],
        };

        let unreachable = executor.dry_run(&scene).await;
        assert_eq!(unreachable.len(), 0);
    }

    #[tokio::test]
    async fn dry_run_some_unreachable() {
        let mut executor = SceneExecutor::new();
        let controller = setup_light_controller();
        executor.register_controller("light", controller);

        let scene = Scene {
            id: "unreachable".to_string(),
            name: "Some Unreachable".to_string(),
            description: "Some devices unreachable".to_string(),
            actions: vec![
                SceneAction {
                    device_id: "light-living-room".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
                SceneAction {
                    device_id: "unknown-device".to_string(),
                    command: "turn_on".to_string(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
            ],
        };

        let unreachable = executor.dry_run(&scene).await;
        assert_eq!(unreachable.len(), 1);
        assert_eq!(unreachable[0], "unknown-device");
    }

    #[tokio::test]
    async fn register_multiple_controllers() {
        let mut executor = SceneExecutor::new();
        let light_controller = setup_light_controller();
        let thermostat_controller = setup_thermostat_controller();

        executor.register_controller("light", light_controller);
        executor.register_controller("thermostat", thermostat_controller);

        assert_eq!(executor.controllers.len(), 2);
    }

    #[tokio::test]
    async fn execute_preset_come_home() {
        let mut executor = SceneExecutor::new();

        let light_controller = Arc::new(MockDeviceController::new("light"));
        light_controller.register_device("light-living-room", "light");

        let thermostat_controller = Arc::new(MockDeviceController::new("thermostat"));
        thermostat_controller.register_device("thermostat-main", "thermostat");

        let speaker_controller = Arc::new(MockDeviceController::new("speaker"));
        speaker_controller.register_device("speaker-living-room", "speaker");

        executor.register_controller("light", light_controller);
        executor.register_controller("thermostat", thermostat_controller);
        executor.register_controller("speaker", speaker_controller);

        let scene = Scene::come_home();
        let result = executor.execute(&scene).await;

        assert_eq!(result.scene_id, "come-home");
        assert!(result.succeeded > 0);
    }

    #[tokio::test]
    async fn execute_preset_leave_home() {
        let mut executor = SceneExecutor::new();

        let light_controller = Arc::new(MockDeviceController::new("light"));
        light_controller.register_device("light-all", "light");

        let lock_controller = Arc::new(MockDeviceController::new("lock"));
        lock_controller.register_device("lock-front-door", "lock");

        let sensor_controller = Arc::new(MockDeviceController::new("sensor"));
        sensor_controller.register_device("security-system", "sensor");

        executor.register_controller("light", light_controller);
        executor.register_controller("lock", lock_controller);
        executor.register_controller("sensor", sensor_controller);

        let scene = Scene::leave_home();
        let result = executor.execute(&scene).await;

        assert_eq!(result.scene_id, "leave-home");
        assert!(result.succeeded > 0);
    }

    #[tokio::test]
    async fn execute_preset_good_night() {
        let mut executor = SceneExecutor::new();

        let light_controller = Arc::new(MockDeviceController::new("light"));
        light_controller.register_device("light-all", "light");

        let lock_controller = Arc::new(MockDeviceController::new("lock"));
        lock_controller.register_device("lock-front-door", "lock");

        let thermostat_controller = Arc::new(MockDeviceController::new("thermostat"));
        thermostat_controller.register_device("thermostat-main", "thermostat");

        executor.register_controller("light", light_controller);
        executor.register_controller("lock", lock_controller);
        executor.register_controller("thermostat", thermostat_controller);

        let scene = Scene::good_night();
        let result = executor.execute(&scene).await;

        assert_eq!(result.scene_id, "good-night");
        assert!(result.succeeded > 0);
    }

    #[tokio::test]
    async fn execute_preset_movie_time() {
        let mut executor = SceneExecutor::new();

        let light_controller = Arc::new(MockDeviceController::new("light"));
        light_controller.register_device("light-living-room", "light");

        let tv_controller = Arc::new(MockDeviceController::new("smart_tv"));
        tv_controller.register_device("tv-living-room", "smart_tv");

        executor.register_controller("light", light_controller);
        executor.register_controller("smart_tv", tv_controller);

        let scene = Scene::movie_time();
        let result = executor.execute(&scene).await;

        assert_eq!(result.scene_id, "movie-time");
        assert!(result.succeeded > 0);
    }
}
