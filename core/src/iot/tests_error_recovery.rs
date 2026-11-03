//! 错误恢复测试 — 异常场景验证

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tokio::sync::RwLock;

    use crate::iot::automation::*;
    use crate::iot::control::{GenericController, MockDeviceController};
    use crate::iot::device_state_cache::DeviceStateCache;
    use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};
    use crate::iot::scene_executor::SceneExecutor;
    use crate::iot::scenes::{Scene, SceneAction, SceneManager};
    use crate::iot::types::DeviceType;

    // 1. 设备离线（未注册）→ 场景部分成功，继续执行剩余
    #[tokio::test]
    async fn scene_partial_failure_continues() {
        let mut executor = SceneExecutor::new();
        let mock = Arc::new(MockDeviceController::new("light"));
        mock.register_device("light-001", "light");
        // "light-999" is NOT registered → will fail
        executor.register_controller("light", mock);

        let scene = Scene {
            id: "partial".into(),
            name: "Partial".into(),
            description: "test".into(),
            actions: vec![
                SceneAction {
                    device_id: "light-001".into(),
                    command: "turn_on".into(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
                SceneAction {
                    device_id: "light-999".into(),
                    command: "turn_on".into(),
                    params: serde_json::json!({}),
                    delay_ms: 0,
                },
            ],
        };

        let result = executor.execute(&scene).await;
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].device_id, "light-999");
    }

    // 2. EventBus 订阅者 drop → 其他订阅者不受影响
    #[tokio::test]
    async fn subscriber_drop_no_impact() {
        let bus = Arc::new(EventBus::new(64));
        let mut rx1 = bus.subscribe();
        {
            let _rx2 = bus.subscribe(); // dropped immediately
        }

        bus.publish(DeviceEvent::new(
            "dev-001",
            DeviceType::Sensor,
            EventKind::StateChanged,
            serde_json::json!({}),
        ));

        let event = rx1.recv().await.unwrap();
        assert_eq!(event.device_id, "dev-001");
    }

    // 3. 无效 cron 表达式 → 该规则跳过，其他规则正常
    #[tokio::test]
    async fn invalid_cron_skipped() {
        let bus = Arc::new(EventBus::new(64));
        let engine = Arc::new(AutomationEngine::new(Arc::clone(&bus)));
        let cache = Arc::new(DeviceStateCache::new());

        // Invalid cron
        let bad_rule = AutomationRule {
            id: "bad-cron".into(),
            name: "Bad cron".into(),
            enabled: true,
            triggers: vec![Trigger::Schedule {
                cron: "not valid cron".into(),
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "should not fire".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        // Valid cron
        let good_rule = AutomationRule {
            id: "good-cron".into(),
            name: "Good cron".into(),
            enabled: true,
            triggers: vec![Trigger::Schedule {
                cron: "* * * * * * *".into(),
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "good fired".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(bad_rule).await.unwrap();
        engine.add_rule(good_rule).await.unwrap();

        crate::iot::cron_scheduler::CronScheduler::check_schedules(&engine, &cache).await;

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].0, "good fired");
    }

    // 4. Controller 返回错误 → 动作失败，下一个动作继续
    #[tokio::test]
    async fn controller_error_continues_next_action() {
        let bus = Arc::new(EventBus::new(64));
        // Controller with no registered devices → all commands fail
        let gc = GenericController::new();
        let engine = AutomationEngine::new(Arc::clone(&bus)).with_controller(Arc::new(gc));

        let rule = AutomationRule {
            id: "err-continue".into(),
            name: "Error continue".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![
                Action::DeviceCommand {
                    device_id: "nonexistent".into(),
                    command: "turn_on".into(),
                    params: serde_json::json!({}),
                },
                Action::Notify {
                    message: "after error".into(),
                    level: NotifyLevel::Info,
                },
            ],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.trigger_rule("err-continue").await.unwrap();

        // Notify should still fire even though DeviceCommand failed
        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].0, "after error");
    }

    // 5. 重复设备发现 → 去重，不重复注册
    #[tokio::test]
    async fn duplicate_discovery_dedup() {
        let path = format!("/tmp/prometheus-err-reg-{}.db", uuid::Uuid::new_v4());
        let registry = Arc::new(crate::iot::registry::DeviceRegistry::new(&path).unwrap());
        let bus = Arc::new(EventBus::new(64));
        let provisioner = crate::iot::auto_provision::AutoProvisioner::new(
            Arc::clone(&registry),
            Arc::clone(&bus),
        );

        let device = crate::iot::discovery::DeviceInfo {
            id: "dup-001".into(),
            name: "Dup Device".into(),
            device_type: "Sensor".into(),
            ip: "10.0.0.1".into(),
            port: 80,
            protocol: "http".into(),
        };

        // Provision same device twice
        provisioner.provision(&device).unwrap();
        provisioner.provision(&device).unwrap();

        let all = registry.list_all().unwrap();
        assert_eq!(all.len(), 1); // INSERT OR REPLACE → still 1
    }

    // 6. RunScene 引用不存在的 scene_id → 不 panic
    #[tokio::test]
    async fn run_nonexistent_scene_no_panic() {
        let bus = Arc::new(EventBus::new(64));
        let scene_mgr = Arc::new(RwLock::new(SceneManager::new()));
        let executor = Arc::new(RwLock::new(SceneExecutor::new()));

        let engine = AutomationEngine::new(Arc::clone(&bus))
            .with_scene_executor(executor)
            .with_scene_manager(scene_mgr);

        let rule = AutomationRule {
            id: "bad-scene".into(),
            name: "Bad scene ref".into(),
            enabled: true,
            triggers: vec![],
            conditions: vec![],
            actions: vec![Action::RunScene {
                scene_id: "nonexistent".into(),
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        // Should not panic
        engine.trigger_rule("bad-scene").await.unwrap();
    }

    // 7. 并发修改规则 + 评估 → 无死锁
    #[tokio::test]
    async fn concurrent_modify_and_evaluate() {
        let bus = Arc::new(EventBus::new(64));
        let engine = Arc::new(AutomationEngine::new(Arc::clone(&bus)));

        // Spawn writer
        let e1 = Arc::clone(&engine);
        let writer = tokio::spawn(async move {
            for i in 0..50 {
                let rule = AutomationRule {
                    id: format!("conc-{}", i),
                    name: format!("Conc {}", i),
                    enabled: true,
                    triggers: vec![],
                    conditions: vec![],
                    actions: vec![],
                    last_triggered: None,
                    cooldown_secs: None,
                };
                let _ = e1.add_rule(rule).await;
            }
        });

        // Spawn reader
        let e2 = Arc::clone(&engine);
        let reader = tokio::spawn(async move {
            for _ in 0..50 {
                let _ = e2.list_rules().await;
            }
        });

        tokio::time::timeout(tokio::time::Duration::from_secs(5), async {
            writer.await.unwrap();
            reader.await.unwrap();
        })
        .await
        .unwrap();
    }

    // 8. EventBus 容量溢出 → Lagged 错误处理，不 panic
    #[tokio::test]
    async fn eventbus_overflow_no_panic() {
        let bus = EventBus::new(4); // Very small buffer
        let mut rx = bus.subscribe();

        // Publish more than capacity
        for i in 0..20 {
            bus.publish(DeviceEvent::new(
                format!("dev-{}", i),
                DeviceType::Sensor,
                EventKind::StateChanged,
                serde_json::json!({}),
            ));
        }

        // Receiver should get Lagged error, then continue
        let mut received = 0;
        loop {
            match rx.recv().await {
                Ok(_) => received += 1,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Expected — continue receiving
                    continue;
                }
                Err(_) => break,
            }
            if received >= 1 {
                break;
            }
        }
        // At least some messages received after lagged
        assert!(received >= 1);
    }

    // 9. 空规则列表 → 事件正常流过，无错误
    #[tokio::test]
    async fn empty_rules_event_flows() {
        let bus = Arc::new(EventBus::new(64));
        let engine = AutomationEngine::new(Arc::clone(&bus));
        engine.start().await;

        bus.publish(DeviceEvent::new(
            "sensor-001",
            DeviceType::Sensor,
            EventKind::StateChanged,
            serde_json::json!({"temperature": 25}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // No rules, no notifications, no panic
        assert!(engine.notifications().await.is_empty());
    }

    // 10. 大 payload 事件 → 正常处理，state_cache 正确合并
    #[tokio::test]
    async fn large_payload_handled() {
        let bus = EventBus::new(64);
        let cache = DeviceStateCache::new();
        cache.start(&bus).await;

        // Build a large payload
        let mut big = serde_json::Map::new();
        for i in 0..100 {
            big.insert(format!("field_{}", i), serde_json::json!(i));
        }

        bus.publish(DeviceEvent::new(
            "big-dev",
            DeviceType::Sensor,
            EventKind::PropertyChanged,
            serde_json::Value::Object(big),
        ));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        let state = cache.get("big-dev").await.unwrap();
        assert_eq!(state.as_object().unwrap().len(), 100);
        assert_eq!(state["field_50"], 50);
    }
}
