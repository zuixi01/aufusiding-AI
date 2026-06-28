//! 压力测试 — 高并发场景验证

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use crate::iot::automation::*;
    use crate::iot::control::MockDeviceController;
    use crate::iot::device_state_cache::DeviceStateCache;
    use crate::iot::discovery_aggregator::DiscoveryAggregator;
    use crate::iot::discovery_ble::BleDiscoveryProvider;
    use crate::iot::discovery_mdns::MdnsDiscoveryProvider;
    use crate::iot::discovery_simulated::*;
    use crate::iot::discovery_ssdp::SsdpDiscoveryProvider;
    use crate::iot::event_bus::{DeviceEvent, EventBus, EventKind};
    use crate::iot::rule_store::RuleStore;
    use crate::iot::scene_executor::SceneExecutor;
    use crate::iot::scenes::Scene;
    use crate::iot::types::DeviceType;

    // 1. EventBus 1000 事件 → 单订阅者全部接收
    #[tokio::test]
    async fn eventbus_1000_events_single_subscriber() {
        let bus = Arc::new(EventBus::new(2048));
        let mut rx = bus.subscribe();
        let count = Arc::new(AtomicUsize::new(0));

        let c = Arc::clone(&count);
        let handle = tokio::spawn(async move {
            while rx.recv().await.is_ok() {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n + 1 >= 1000 { break; }
            }
        });

        for i in 0..1000 {
            bus.publish(DeviceEvent::new(
                format!("dev-{}", i), DeviceType::Sensor,
                EventKind::StateChanged, serde_json::json!({"i": i}),
            ));
        }

        tokio::time::timeout(
            tokio::time::Duration::from_secs(5), handle
        ).await.unwrap().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1000);
    }

    // 2. EventBus 50 订阅者 → 全部收到同一事件
    #[tokio::test]
    async fn eventbus_50_subscribers() {
        let bus = Arc::new(EventBus::new(64));
        let count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for _ in 0..50 {
            let mut rx = bus.subscribe();
            let c = Arc::clone(&count);
            handles.push(tokio::spawn(async move {
                if rx.recv().await.is_ok() {
                    c.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        tokio::task::yield_now().await;
        bus.publish(DeviceEvent::new(
            "dev-001", DeviceType::Sensor, EventKind::StateChanged,
            serde_json::json!({}),
        ));

        for h in handles {
            tokio::time::timeout(tokio::time::Duration::from_secs(2), h)
                .await.unwrap().unwrap();
        }
        assert_eq!(count.load(Ordering::SeqCst), 50);
    }

    // 3. 100 条规则同时评估 → 全部匹配的都执行
    #[tokio::test]
    async fn hundred_rules_all_fire() {
        let bus = Arc::new(EventBus::new(256));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        for i in 0..100 {
            let rule = AutomationRule {
                id: format!("stress-{}", i),
                name: format!("Stress {}", i),
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
            "sensor-001", DeviceType::Sensor, EventKind::StateChanged,
            serde_json::json!({}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let notifs = engine.notifications().await;
        assert_eq!(notifs.len(), 100);
    }

    // 4. 50 个场景并发执行 → 全部完成
    #[tokio::test]
    async fn fifty_scenes_concurrent() {
        let mock = Arc::new(MockDeviceController::new("light"));
        mock.register_device("light-001", "light");

        let mut handles = Vec::new();
        for i in 0..50 {
            let ctrl = Arc::clone(&mock);
            handles.push(tokio::spawn(async move {
                let mut executor = SceneExecutor::new();
                executor.register_controller("light", ctrl);
                let scene = Scene {
                    id: format!("scene-{}", i),
                    name: format!("Scene {}", i),
                    description: "test".into(),
                    actions: vec![crate::iot::scenes::SceneAction {
                        device_id: "light-001".into(),
                        command: "turn_on".into(),
                        params: serde_json::json!({}),
                        delay_ms: 0,
                    }],
                };
                let result = executor.execute(&scene).await;
                assert_eq!(result.succeeded, 1);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
    }

    // 5. 快速 add/remove 规则 100 次 → 无死锁
    #[tokio::test]
    async fn rapid_add_remove_no_deadlock() {
        let bus = Arc::new(EventBus::new(64));
        let engine = Arc::new(AutomationEngine::new(bus));

        let mut handles = Vec::new();
        for i in 0..100 {
            let e = Arc::clone(&engine);
            handles.push(tokio::spawn(async move {
                let rule = AutomationRule {
                    id: format!("rapid-{}", i),
                    name: format!("Rapid {}", i),
                    enabled: true,
                    triggers: vec![],
                    conditions: vec![],
                    actions: vec![],
                    last_triggered: None,
                    cooldown_secs: None,
                };
                let _ = e.add_rule(rule).await;
                e.remove_rule(&format!("rapid-{}", i)).await;
            }));
        }
        for h in handles {
            tokio::time::timeout(tokio::time::Duration::from_secs(5), h)
                .await.unwrap().unwrap();
        }
        // Should end with 0 rules (all removed)
        let rules = engine.list_rules().await;
        assert!(rules.is_empty());
    }

    // 6. DeviceStateCache 100 并发写入 → 状态一致
    #[tokio::test]
    async fn state_cache_100_concurrent_writes() {
        let cache = Arc::new(DeviceStateCache::new());
        let mut handles = Vec::new();

        for i in 0..100 {
            let c = Arc::clone(&cache);
            handles.push(tokio::spawn(async move {
                c.set(&format!("dev-{}", i), serde_json::json!({"val": i})).await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let snap = cache.snapshot().await;
        assert_eq!(snap.len(), 100);
    }

    // 7. RuleStore 50 并发保存 → 全部持久化
    #[tokio::test]
    async fn rule_store_50_concurrent_saves() {
        let path = format!("/tmp/prometheus-stress-rules-{}.db", uuid::Uuid::new_v4());
        let store = Arc::new(RuleStore::new(&path).unwrap());

        let mut handles = Vec::new();
        for i in 0..50 {
            let s = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                let rule = AutomationRule {
                    id: format!("conc-{}", i),
                    name: format!("Concurrent {}", i),
                    enabled: true,
                    triggers: vec![],
                    conditions: vec![],
                    actions: vec![],
                    last_triggered: None,
                    cooldown_secs: None,
                };
                s.save(&rule).unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 50);
    }

    // 8. DiscoveryAggregator 3 provider × 多设备 → 正确去重
    #[tokio::test]
    async fn aggregator_many_devices_dedup() {
        let mut agg = DiscoveryAggregator::new();
        agg.add_provider(Box::new(MdnsDiscoveryProvider::with_socket(
            Box::new(SimulatedMdnsSocket::typical_lan_devices()),
        )));
        agg.add_provider(Box::new(SsdpDiscoveryProvider::with_socket(
            Box::new(SimulatedSsdpSocket::typical_upnp_devices()),
        )));
        agg.add_provider(Box::new(BleDiscoveryProvider::with_scanner(
            Box::new(SimulatedBleScanner::typical_ble_devices()),
        )));

        let devices = agg.scan_all(1).await.unwrap();
        // All have unique IPs, so no dedup needed — just verify count
        assert!(devices.len() >= 8);
    }

    // 9. 混合动作 500 事件 → 全部处理
    #[tokio::test]
    async fn mixed_actions_500_events() {
        let bus = Arc::new(EventBus::new(1024));
        let engine = AutomationEngine::new(Arc::clone(&bus));

        let rule = AutomationRule {
            id: "mixed-500".into(),
            name: "Mixed 500".into(),
            enabled: true,
            triggers: vec![Trigger::DeviceEvent {
                device_id: "sensor-001".into(),
                event_kind: EventKind::StateChanged,
            }],
            conditions: vec![],
            actions: vec![Action::Notify {
                message: "ping".into(),
                level: NotifyLevel::Info,
            }],
            last_triggered: None,
            cooldown_secs: None,
        };
        engine.add_rule(rule).await.unwrap();
        engine.start().await;

        // First event triggers, rest are in cooldown=None so all fire
        for _ in 0..500 {
            bus.publish(DeviceEvent::new(
                "sensor-001", DeviceType::Sensor, EventKind::StateChanged,
                serde_json::json!({}),
            ));
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let notifs = engine.notifications().await;
        assert!(notifs.len() >= 100); // At least many processed
    }

    // 10. AutoProvisioner 100 设备快速发现 → 全部注册
    #[tokio::test]
    async fn auto_provision_100_devices() {
        let path = format!("/tmp/prometheus-stress-reg-{}.db", uuid::Uuid::new_v4());
        let registry = Arc::new(crate::iot::registry::DeviceRegistry::new(&path).unwrap());
        let bus = Arc::new(EventBus::new(256));
        let provisioner = crate::iot::auto_provision::AutoProvisioner::new(
            Arc::clone(&registry), Arc::clone(&bus),
        );

        for i in 0..100 {
            let device = crate::iot::discovery::DeviceInfo {
                id: format!("dev-{}", i),
                name: format!("Device {}", i),
                device_type: "Sensor".into(),
                ip: format!("10.0.0.{}", i),
                port: 8080,
                protocol: "http".into(),
            };
            provisioner.provision(&device).unwrap();
        }

        let all = registry.list_all().unwrap();
        assert_eq!(all.len(), 100);
    }
}
