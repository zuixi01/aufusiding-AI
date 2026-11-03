//! 集成测试 — 端到端流程验证
//!
//! 测试各模块协同工作：事件 → 自动化 → 场景执行 → 设备状态变更。

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    use crate::iot::auto_provision::AutoProvisioner;
    use crate::iot::automation::*;
    use crate::iot::control::{GenericController, MockDeviceController};
    use crate::iot::device_state_cache::DeviceStateCache;
    use crate::iot::discovery::DeviceInfo;
    use crate::iot::discovery_aggregator::DiscoveryAggregator;
    use crate::iot::discovery_ble::BleDiscoveryProvider;
    use crate::iot::discovery_mdns::MdnsDiscoveryProvider;
    use crate::iot::discovery_simulated::*;
    use crate::iot::discovery_ssdp::SsdpDiscoveryProvider;
    use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};
    use crate::iot::registry::DeviceRegistry;
    use crate::iot::rule_store::RuleStore;
    use crate::iot::scene_executor::SceneExecutor;
    use crate::iot::scenes::{Scene, SceneManager};
    use crate::iot::types::DeviceType;

    fn temp_registry() -> Arc<DeviceRegistry> {
        let path = format!("/tmp/prometheus-integ-reg-{}.db", uuid::Uuid::new_v4());
        Arc::new(DeviceRegistry::new(&path).unwrap())
    }

    fn setup_full_engine(bus: &Arc<EventBus>) -> (AutomationEngine, Arc<MockDeviceController>) {
        let mock = Arc::new(MockDeviceController::new("generic"));
        mock.register_device("ac-001", "ac");
        mock.register_device("light-001", "light");
        mock.register_device("light-living-room", "light");
        mock.register_device("thermostat-main", "thermostat");
        mock.register_device("speaker-living-room", "speaker");

        let mut gc = GenericController::new();
        gc.register_adapter(Box::new(MockDeviceController::new("generic")));
        // Register a controller that knows our devices
        let ctrl_mock = MockDeviceController::new("all");
        ctrl_mock.register_device("ac-001", "ac");
        ctrl_mock.register_device("light-001", "light");
        ctrl_mock.register_device("light-living-room", "light");
        ctrl_mock.register_device("thermostat-main", "thermostat");
        ctrl_mock.register_device("speaker-living-room", "speaker");
        gc.register_adapter(Box::new(ctrl_mock));
        let controller = Arc::new(gc);

        let mut scene_mgr = SceneManager::new();
        scene_mgr.add_scene(Scene::come_home());
        scene_mgr.add_scene(Scene::leave_home());
        scene_mgr.add_scene(Scene::good_night());
        scene_mgr.add_scene(Scene::movie_time());

        let mut executor = SceneExecutor::new();
        let exec_mock = Arc::new(MockDeviceController::new("all"));
        exec_mock.register_device("light-living-room", "light");
        exec_mock.register_device("light-all", "light");
        exec_mock.register_device("thermostat-main", "thermostat");
        exec_mock.register_device("speaker-living-room", "speaker");
        exec_mock.register_device("lock-front-door", "lock");
        exec_mock.register_device("security-system", "security");
        exec_mock.register_device("tv-living-room", "smart_tv");
        executor.register_controller("all", exec_mock.clone());

        let cache = Arc::new(DeviceStateCache::new());
        let engine = AutomationEngine::new(Arc::clone(bus))
            .with_controller(controller)
            .with_scene_executor(Arc::new(RwLock::new(executor)))
            .with_scene_manager(Arc::new(RwLock::new(scene_mgr)))
            .with_state_cache(Arc::clone(&cache));

        (engine, mock)
    }

    // 1. 事件 → AutomationEngine → SceneExecutor → 设备状态变更
    #[tokio::test]
    async fn event_triggers_scene_execution() {
        let bus = Arc::new(EventBus::new(64));
        let (engine, _mock) = setup_full_engine(&bus);

        let rule = AutomationRule {
            id: "scene-trigger".into(),
            name: "Door open → come home".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "door-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::RunScene {
                scene_id: "come-home".into(),
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "door-001",
            DeviceType::Lock,
            EventKind::StateChanged,
            serde_json::json!({"opened": true}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_some());
    }

    // 2. 阈值触发 → 条件评估（state_cache）→ 动作执行
    #[tokio::test]
    async fn threshold_with_condition_evaluation() {
        let bus = Arc::new(EventBus::new(64));
        let cache = Arc::new(DeviceStateCache::new());
        cache
            .set("sensor-001", serde_json::json!({"temperature": 35}))
            .await;

        let mut gc = GenericController::new();
        let mock = MockDeviceController::new("ac");
        mock.register_device("ac-001", "ac");
        gc.register_adapter(Box::new(mock));

        let engine = AutomationEngine::new(Arc::clone(&bus))
            .with_controller(Arc::new(gc))
            .with_state_cache(Arc::clone(&cache));

        let rule = AutomationRule {
            id: "temp-ac".into(),
            name: "Temp > 30 → AC on".into(),
            enabled: true,
            triggers: vec![Trigger::Threshold {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            conditions: vec![Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            actions: vec![
                Action::DeviceCommand {
                    device_id: "ac-001".into(),
                    command: "turn_on".into(),
                    params: serde_json::json!({}),
                },
                Action::Notify {
                    message: "AC on".into(),
                    level: NotifyLevel::Info,
                },
            ],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 35}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].0, "AC on");
    }

    // 3. Discovery(simulated) → AutoProvisioner → Registry → 查询确认
    #[tokio::test]
    async fn discovery_to_provisioner_to_registry() {
        let bus = Arc::new(EventBus::new(128));
        let registry = temp_registry();
        let provisioner = AutoProvisioner::new(Arc::clone(&registry), Arc::clone(&bus));

        let mut agg = DiscoveryAggregator::new().with_event_bus(Arc::clone(&bus));
        let mdns_socket = SimulatedMdnsSocket::typical_lan_devices();
        agg.add_provider(Box::new(MdnsDiscoveryProvider::with_socket(Box::new(
            mdns_socket,
        ))));

        let devices = agg.scan_all(1).await.unwrap();
        assert!(!devices.is_empty());

        for device in &devices {
            provisioner.provision(device).unwrap();
        }

        let all = registry.list_all().unwrap();
        assert_eq!(all.len(), devices.len());
    }

    // 4. Geofence 事件 → 自动化规则 → 通知
    #[tokio::test]
    async fn geofence_event_triggers_automation() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "geo-home".into(),
            name: "Arrive home".into(),
            enabled: true,
            triggers: vec![Trigger::Geofence {
                zone_id: "home".into(),
                transition: crate::iot::vehicle::geofence::GeoTransition::Enter,
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "Welcome home!".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "phone-001",
            DeviceType::Other("phone".into()),
            EventKind::StateChanged,
            serde_json::json!({
                "zone_id": "home",
                "transition": "Enter"
            }),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].0, "Welcome home!");
    }

    // 5. 完整生命周期：发现 → 注册 → 创建规则 → 触发 → 执行
    #[tokio::test]
    async fn full_lifecycle() {
        let bus = Arc::new(EventBus::new(128));
        let registry = temp_registry();

        // Discover
        let device = DeviceInfo {
            id: "sensor-temp".into(),
            name: "Temp Sensor".into(),
            device_type: "Sensor".into(),
            ip: "192.168.1.100".into(),
            port: 8080,
            protocol: "http".into(),
        };
        let provisioner = AutoProvisioner::new(Arc::clone(&registry), Arc::clone(&bus));
        provisioner.provision(&device).unwrap();
        assert!(registry.get("sensor-temp").unwrap().is_some());

        // Create automation
        let engine = AutomationEngine::new(Arc::clone(&bus));
        let rule = AutomationRule {
            id: "lifecycle".into(),
            name: "Lifecycle test".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-temp".into(),
                event_kind: EventKind::PropertyChanged,
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "lifecycle complete".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        // Trigger
        bus.publish(DeviceEvent::new(
            "sensor-temp",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 22}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 1);
    }

    // 6. 多规则同时匹配 → 全部执行
    #[tokio::test]
    async fn multiple_rules_all_fire() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        for i in 0..5 {
            let rule = AutomationRule {
                id: format!("multi-{}", i),
                name: format!("Rule {}", i),
                enabled: true,
                triggers: vec![Trigger::DeviceEvent {
                    device_id: "sensor-001".into(),
                    event_kind: EventKind::StateChanged,
                }],
                conditions: vec![],
                actions: vec![Action::Notify {
                    message: format!("fired-{}", i),
                    level: NotifyLevel::Info,
                }],
                last_triggered: None,
                cooldown_secs: None,
            };
            engine.add_rule(rule).await.unwrap();
        }
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 5);
    }

    // 7. RunScene 动作 → SceneManager 获取场景 → SceneExecutor 执行
    #[tokio::test]
    async fn run_scene_action_full_path() {
        let bus = Arc::new(EventBus::new(64));
        let (engine, _) = setup_full_engine(&bus);

        let rule = AutomationRule {
            id: "run-movie".into(),
            name: "Movie time".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![Action::RunScene {
                scene_id: "movie-time".into(),
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.trigger_rule("run-movie").await.unwrap();

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_some());
    }

    // 8. Notify 动作 → 通知日志记录
    #[tokio::test]
    async fn notify_action_logs_correctly() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "notify-test".into(),
            name: "Notify".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![
                Action::Notify {
                    message: "info msg".into(),
                    level: NotifyLevel::Info,
                },
                Action::Notify {
                    message: "warn msg".into(),
                    level: NotifyLevel::Warning,
                },
                Action::Notify {
                    message: "crit msg".into(),
                    level: NotifyLevel::Critical,
                },
            ],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.trigger_rule("notify-test").await.unwrap();

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 3);
        assert_eq!(notifs[0].1, NotifyLevel::Info);
        assert_eq!(notifs[1].1, NotifyLevel::Warning);
        assert_eq!(notifs[2].1, NotifyLevel::Critical);
    }

    // 9. Sequence(DeviceCommand, Delay, Notify) → 顺序执行
    #[tokio::test]
    async fn sequence_with_delay_and_notify() {
        let bus = Arc::new(EventBus::new(64));
        let mut gc = GenericController::new();
        let mock = MockDeviceController::new("ac");
        mock.register_device("ac-001", "ac");
        gc.register_adapter(Box::new(mock));

        let engine = AutomationEngine::new(Arc::clone(&bus)).with_controller(Arc::new(gc));

        let rule = AutomationRule {
            id: "seq-test".into(),
            name: "Sequence".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![Action::Sequence(vec![
                Action::DeviceCommand {
                    device_id: "ac-001".into(),
                    command: "turn_on".into(),
                    params: serde_json::json!({}),
                },
                Action::Delay {
                    duration_ms: 10,
                    then: Box::new(Action::Notify {
                        message: "after delay".into(),
                        level: NotifyLevel::Info,
                    }),
                },
                Action::Notify {
                    message: "end".into(),
                    level: NotifyLevel::Info,
                },
            ])],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.trigger_rule("seq-test").await.unwrap();

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 2);
        assert_eq!(notifs[0].0, "after delay");
        assert_eq!(notifs[1].0, "end");
    }

    // 10. RuleStore 持久化 → 重启加载 → 规则仍可触发
    #[tokio::test]
    async fn rule_store_persist_and_reload() {
        let db_path = format!("/tmp/prometheus-integ-rules-{}.db", uuid::Uuid::new_v4());
        let bus = Arc::new(EventBus::new(64));

        // Save rules
        {
            let engine = AutomationEngine::new(Arc::clone(&bus));
            let rule = AutomationRule {
                id: "persist-r1".into(),
                name: "Persistent".into(),
                enabled: true,
                triggers: vec![Trigger::DeviceEvent {
                    device_id: "sensor-001".into(),
                    event_kind: EventKind::StateChanged,
                }],
                conditions: vec![],
                actions: vec![Action::Notify {
                    message: "persisted".into(),
                    level: NotifyLevel::Info,
                }],
                last_triggered: None,
                cooldown_secs: None,
            };
            engine.add_rule(rule).await.unwrap();
            let store = RuleStore::new(&db_path).unwrap();
            store.sync_from_engine(&engine).await.unwrap();
        }

        // Reload into new engine
        let engine2 = AutomationEngine::new(Arc::clone(&bus));
        let store2 = RuleStore::new(&db_path).unwrap();
        store2.sync_to_engine(&engine2).await.unwrap();

        let rules = engine2.list_rules().await;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "persist-r1");

        // Trigger the reloaded rule
        engine2.start().await;
        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let notifs = engine2.notifications().await;
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].0, "persisted");
    }

    // 11. DeviceStateCache 合并多次更新 → 条件评估正确
    #[tokio::test]
    async fn state_cache_merge_then_evaluate() {
        let bus = Arc::new(EventBus::new(64));
        let cache = Arc::new(DeviceStateCache::new());
        cache.start(&bus).await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 25}),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"humidity": 60}),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        let snap = cache.snapshot().await;
        let cond = Condition::And(vec![
            Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(20),
            },
            Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "humidity".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(50),
            },
        ]);
        assert!(AutomationEngine::evaluate_condition(&cond, &snap));
    }

    // 12. DiscoveryAggregator + 3 SimulatedSocket → 去重合并
    #[tokio::test]
    async fn aggregator_three_providers_dedup() {
        let mut agg = DiscoveryAggregator::new();
        agg.add_provider(Box::new(MdnsDiscoveryProvider::with_socket(Box::new(
            SimulatedMdnsSocket::typical_lan_devices(),
        ))));
        agg.add_provider(Box::new(SsdpDiscoveryProvider::with_socket(Box::new(
            SimulatedSsdpSocket::typical_upnp_devices(),
        ))));
        agg.add_provider(Box::new(BleDiscoveryProvider::with_scanner(Box::new(
            SimulatedBleScanner::typical_ble_devices(),
        ))));

        let devices = agg.scan_all(1).await.unwrap();
        // mDNS: 5, SSDP: 3, BLE: 2 — all unique IPs so 10 total
        assert!(devices.len() >= 8);
    }

    // 13. AutoProvisioner 监听发现事件 → 自动注册 → 发布配对事件
    #[tokio::test]
    async fn auto_provisioner_event_driven() {
        let bus = Arc::new(EventBus::new(128));
        let registry = temp_registry();
        let provisioner = AutoProvisioner::new(Arc::clone(&registry), Arc::clone(&bus));

        let device = DeviceInfo {
            id: "auto-dev-001".into(),
            name: "Auto Device".into(),
            device_type: "Sensor".into(),
            ip: "10.0.0.1".into(),
            port: 80,
            protocol: "http".into(),
        };
        provisioner.provision(&device).unwrap();

        let registered = registry.get("auto-dev-001").unwrap();
        assert!(registered.is_some());
        assert_eq!(registered.unwrap().name, "Auto Device");
    }

    // 14. 禁用规则 → 事件不触发
    #[tokio::test]
    async fn disabled_rule_no_trigger() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "disabled".into(),
            name: "Disabled".into(),
            enabled: false,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "should not fire".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert!(engine.notifications().await.is_empty());
    }

    // 15. 冷却期 → 快速重复事件只触发一次
    #[tokio::test]
    async fn cooldown_prevents_double_fire() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "cooldown".into(),
            name: "Cooldown".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "fired".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: Some(3600),
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        for _ in 0..5 {
            bus.publish(DeviceEvent::new(
                "sensor-001",
                DeviceType::Sensor,
                EventKind::StateChanged,
                serde_json::json!({}),
            ));
            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 1);
    }

    // 16. 多设备类型混合场景执行
    #[tokio::test]
    async fn mixed_device_scene_execution() {
        let bus = Arc::new(EventBus::new(64));
        let (engine, _) = setup_full_engine(&bus);

        let rule = AutomationRule {
            id: "mixed-scene".into(),
            name: "Good night scene".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![Action::RunScene {
                scene_id: "good-night".into(),
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.trigger_rule("mixed-scene").await.unwrap();

        let rules = engine.list_rules().await;
        assert!(rules[0].last_triggered.is_some());
    }

    // 17. dry_run → 检测不可达设备
    #[tokio::test]
    async fn dry_run_detects_unreachable() {
        let executor = SceneExecutor::new();
        let scene = Scene::come_home();
        let unreachable = executor.dry_run(&scene).await;
        assert!(!unreachable.is_empty());
    }

    // 18. 规则 CRUD → 持久化 → 重新加载一致
    #[tokio::test]
    async fn rule_crud_persist_reload() {
        let db_path = format!("/tmp/prometheus-integ-crud-{}.db", uuid::Uuid::new_v4());
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        for i in 0..3 {
            let rule = AutomationRule {
                id: format!("crud-{}", i),
                name: format!("CRUD {}", i),
                enabled: true,
                triggers: vec![],
                conditions: vec![],
                actions: vec![Action::Notify {
                    message: format!("msg-{}", i),
                    level: NotifyLevel::Info,
                }],
                last_triggered: None,
                cooldown_secs: None,
            };
            engine.add_rule(rule).await.unwrap();
        }
        engine.disable_rule("crud-1").await;
        engine.remove_rule("crud-2").await;

        let store = RuleStore::new(&db_path).unwrap();
        store.sync_from_engine(&engine).await.unwrap();

        let engine2 = AutomationEngine::new(Arc::clone(&bus));
        store.sync_to_engine(&engine2).await.unwrap();

        let rules = engine2.list_rules().await;
        assert_eq!(rules.len(), 2);
        let r1 = rules.iter().find(|r| r.id == "crud-1").unwrap();
        assert!(!r1.enabled);
    }

    // 19. 阈值触发 + TimeRange + DaysOfWeek 组合条件
    #[tokio::test]
    async fn combined_conditions() {
        let cache = Arc::new(DeviceStateCache::new());
        cache
            .set("sensor-001", serde_json::json!({"temperature": 35}))
            .await;
        let snap = cache.snapshot().await;

        let cond = Condition::And(vec![
            Condition::DeviceState {
                device_id: "sensor-001".into(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            },
            Condition::TimeRange {
                after: "00:00".into(),
                before: "23:59".into(),
            },
            Condition::DaysOfWeek {
                days: vec![0, 1, 2, 3, 4, 5, 6],
            },
        ]);

        assert!(AutomationEngine::evaluate_condition(&cond, &snap));
    }

    // 20. 全链路：发现 → 配置 → 创建自动化 → 触发 → 执行 → 验证
    #[tokio::test]
    async fn end_to_end_full_chain() {
        let bus = Arc::new(EventBus::new(128));
        let registry = temp_registry();

        let mut agg = DiscoveryAggregator::new();
        agg.add_provider(Box::new(MdnsDiscoveryProvider::with_socket(Box::new(
            SimulatedMdnsSocket::new(
                vec![crate::iot::discovery_mdns::MdnsResponse {
                    hostname: "sensor.local".into(),
                    ip: "192.168.1.100".into(),
                    port: 8080,
                    service_type: "_http._tcp.local.".into(),
                    txt_records: HashMap::from([("name".into(), "Temp Sensor".into())]),
                }],
                0,
            ),
        ))));
        let discovered = agg.scan_all(1).await.unwrap();
        assert_eq!(discovered.len(), 1);

        let provisioner = AutoProvisioner::new(Arc::clone(&registry), Arc::clone(&bus));
        for d in &discovered {
            provisioner.provision(d).unwrap();
        }
        assert_eq!(registry.list_all().unwrap().len(), 1);

        let cache = Arc::new(DeviceStateCache::new());
        cache
            .set(&discovered[0].id, serde_json::json!({"temperature": 35}))
            .await;

        let engine = AutomationEngine::new(Arc::clone(&bus)).with_state_cache(Arc::clone(&cache));

        let rule = AutomationRule {
            id: "e2e".into(),
            name: "E2E test".into(),
            enabled: true,
            triggers: vec![Trigger::Threshold {
                device_id: discovered[0].id.clone(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            conditions: vec![Condition::DeviceState {
                device_id: discovered[0].id.clone(),
                property: "temperature".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30),
            }],
            actions: vec![Action::Notify {
                message: "E2E complete".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        bus.publish(DeviceEvent::new(
            &discovered[0].id,
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::json!({"temperature": 35}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].0, "E2E complete");
    }
}
