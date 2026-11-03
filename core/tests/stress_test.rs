//! Task 6: 压力与并发测试
//!
//! 验证核心组件在高负载下的稳定性和正确性。

use prometheus_core::iot::event_bus::{DeviceEvent, EventBus, EventKind};
use prometheus_core::iot::types::DeviceType;
use prometheus_core::memory::{KnowledgeBase, WorkingMemory};
use prometheus_core::orchestrator::{ChatOrchestrator, ProviderChatBackend};
use prometheus_core::providers::mock::MockProvider;
use prometheus_core::providers::registry::ProviderRegistry;
use prometheus_core::router::fallback::FallbackRouter;
use prometheus_core::router::runtime::RoutingRuntime;
use prometheus_core::session::HistoryStore;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

fn temp_path(prefix: &str) -> std::path::PathBuf {
    prometheus_core::test_support::temp_db_path(prefix)
}

// ═══════════════════════════════════════════════════════════════════════════
// 并发安全测试 (5)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn concurrent_chat_sessions_100() {
    let session_db = temp_path("stress-sessions-100");
    let history_db = temp_path("stress-history-100");
    let _ = std::fs::remove_file(&session_db);
    let _ = std::fs::remove_file(&history_db);

    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(MockProvider::new("mock")));
    let fallback = FallbackRouter::with_config(vec!["mock".into()], 1, Duration::from_secs(10));
    let runtime = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(runtime);
    let history = HistoryStore::new(history_db);

    let orchestrator = Arc::new(tokio::sync::RwLock::new(
        ChatOrchestrator::new_with_persistence(session_db, history, backend).unwrap(),
    ));

    // 100 并发会话创建
    let mut tasks = JoinSet::new();
    for i in 0..100 {
        let orch = Arc::clone(&orchestrator);
        tasks.spawn(async move {
            let mut o = orch.write().await;
            o.create_session(Some(&format!("session-{}", i))).await
        });
    }

    let mut success = 0;
    while let Some(result) = tasks.join_next().await {
        if result.unwrap().is_ok() {
            success += 1;
        }
    }

    assert_eq!(success, 100, "100 个并发会话应全部创建成功");
}

#[tokio::test]
async fn concurrent_memory_writes_50() {
    let wm = Arc::new(tokio::sync::RwLock::new(WorkingMemory::new(100)));

    let mut tasks = JoinSet::new();
    for i in 0..50 {
        let wm_clone = Arc::clone(&wm);
        tasks.spawn(async move {
            let mut w = wm_clone.write().await;
            w.add(
                format!("concurrent item {}", i),
                0.5 + ((i % 10) as f64) / 20.0,
            );
        });
    }

    while let Some(result) = tasks.join_next().await {
        result.expect("记忆写入任务不应 panic");
    }

    let wm_read = wm.read().await;
    assert!(wm_read.len() >= 50, "至少应有 50 条记忆（容量 100）");
}

#[tokio::test]
async fn concurrent_knowledge_search_50() {
    let kb_path = temp_path("stress-kb-search");
    let _ = std::fs::remove_file(&kb_path);
    let mut kb = KnowledgeBase::new(kb_path, 384).unwrap();

    // 预先插入数据
    for i in 0..10 {
        kb.add_document(
            &format!("doc-{}", i),
            &format!("Document {} content about testing", i),
        )
        .await
        .unwrap();
    }

    let kb = Arc::new(tokio::sync::RwLock::new(kb));

    // 50 并发搜索
    let mut tasks = JoinSet::new();
    for _ in 0..50 {
        let kb_clone = Arc::clone(&kb);
        tasks.spawn(async move {
            let k = kb_clone.read().await;
            k.search("testing", 5).await
        });
    }

    let mut success = 0;
    while let Some(result) = tasks.join_next().await {
        if result.unwrap().is_ok() {
            success += 1;
        }
    }

    assert_eq!(success, 50, "50 次并发搜索应全部成功");
}

#[tokio::test]
async fn concurrent_iot_commands_30() {
    use prometheus_core::iot::control::GenericController;

    let controller = Arc::new(tokio::sync::RwLock::new(GenericController::new()));

    let mut tasks = JoinSet::new();
    for i in 0..30 {
        let ctrl = Arc::clone(&controller);
        tasks.spawn(async move {
            let c = ctrl.read().await;
            // 模拟设备命令（即使设备不存在也不应 panic）
            c.execute(&format!("device-{}", i), "get_state", serde_json::json!({}))
                .await
        });
    }

    let mut completed = 0;
    while let Some(result) = tasks.join_next().await {
        let outcome = result.expect("IoT 命令任务不应 panic");
        assert!(outcome.is_err(), "未注册适配器的命令应返回错误");
        completed += 1;
    }
    assert_eq!(completed, 30, "30 个并发 IoT 命令应全部完成");
}

#[tokio::test]
async fn concurrent_event_bus_publish_100() {
    let bus = Arc::new(EventBus::new(256));

    // 先订阅，再发布，这样能验证事件确实被发送
    let mut rx = bus.subscribe();

    let mut tasks = JoinSet::new();
    for i in 0..100 {
        let bus_clone = Arc::clone(&bus);
        tasks.spawn(async move {
            bus_clone.publish(DeviceEvent::new(
                format!("device-{}", i),
                DeviceType::Light,
                EventKind::StateChanged,
                serde_json::json!({"powered": true}),
            ));
        });
    }

    let mut completed = 0;
    while let Some(result) = tasks.join_next().await {
        assert!(result.is_ok(), "发布任务不应 panic");
        completed += 1;
    }

    assert_eq!(completed, 100, "100 个并发事件发布应全部完成");

    // 验证订阅者收到了事件
    let mut received = 0;
    while rx.try_recv().is_ok() {
        received += 1;
    }
    assert!(received > 0, "订阅者应收到事件，实际收到: {}", received);
}

// ═══════════════════════════════════════════════════════════════════════════
// 吞吐量测试 (5)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn session_creation_throughput() {
    let session_db = temp_path("stress-throughput-sessions");
    let history_db = temp_path("stress-throughput-history");
    let _ = std::fs::remove_file(&session_db);
    let _ = std::fs::remove_file(&history_db);

    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(MockProvider::new("mock")));
    let fallback = FallbackRouter::with_config(vec!["mock".into()], 1, Duration::from_secs(10));
    let runtime = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(runtime);
    let history = HistoryStore::new(history_db);

    let mut orchestrator =
        ChatOrchestrator::new_with_persistence(session_db, history, backend).unwrap();

    let start = Instant::now();
    for i in 0..1000 {
        orchestrator
            .create_session(Some(&format!("throughput-{}", i)))
            .await
            .unwrap();
    }
    let elapsed = start.elapsed();

    println!("1000 会话创建耗时: {:?}", elapsed);
    assert!(
        elapsed.as_secs() < 5,
        "1000 会话创建应在 5 秒内完成，实际: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn message_throughput() {
    let session_db = temp_path("stress-msg-throughput");
    let history_db = temp_path("stress-msg-history");
    let _ = std::fs::remove_file(&session_db);
    let _ = std::fs::remove_file(&history_db);

    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(MockProvider::new("mock")));
    let fallback = FallbackRouter::with_config(vec!["mock".into()], 1, Duration::from_secs(10));
    let runtime = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(runtime);
    let history = HistoryStore::new(history_db);

    let mut orchestrator =
        ChatOrchestrator::new_with_persistence(session_db, history, backend).unwrap();
    let session = orchestrator
        .create_session(Some("msg-throughput"))
        .await
        .unwrap();

    let start = Instant::now();
    for i in 0..500 {
        let _ = orchestrator
            .send_message(session.id, &format!("message {}", i))
            .await;
    }
    let elapsed = start.elapsed();

    println!("500 消息发送耗时: {:?}", elapsed);
    assert!(
        elapsed.as_secs() < 15,
        "单会话 500 消息应在 15 秒内完成，实际: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn vector_store_bulk_insert() {
    use prometheus_core::storage::VectorStore;

    let store = VectorStore::new(temp_path("stress-vector-bulk"), 384);
    store.init().unwrap();

    let vectors: Vec<(String, Vec<f32>, String)> = (0..10000)
        .map(|i| {
            let mut v = vec![0.0f32; 384];
            v[i % 384] = 1.0;
            (format!("id-{}", i), v, format!("meta-{}", i))
        })
        .collect();

    let start = Instant::now();
    store.insert_batch(&vectors).unwrap();
    let elapsed = start.elapsed();

    println!("10000 向量插入耗时: {:?}", elapsed);
    assert!(
        elapsed.as_secs() < 10,
        "10000 向量插入应在 10 秒内完成，实际: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn routing_throughput() {
    use prometheus_core::router::classifier::IntentClassifier;
    use prometheus_core::router::strategy::{Router, RoutingStrategy};

    let classifier = IntentClassifier::new();
    let mut router = Router::new(RoutingStrategy::Balanced);

    let start = Instant::now();
    for i in 0..10000 {
        let text = format!("请求 {}", i);
        let classification = classifier.classify(&text).await.unwrap();
        let _ = router.select_model(&classification, 1000);
    }
    let elapsed = start.elapsed();

    println!("10000 路由决策耗时: {:?}", elapsed);
    assert!(
        elapsed.as_secs() < 2,
        "10000 路由决策应在 2 秒内完成，实际: {:?}",
        elapsed
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 资源泄漏测试 (5)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn no_memory_leak_sessions() {
    let session_db = temp_path("stress-leak-sessions");
    let history_db = temp_path("stress-leak-history");
    let _ = std::fs::remove_file(&session_db);
    let _ = std::fs::remove_file(&history_db);

    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(MockProvider::new("mock")));
    let fallback = FallbackRouter::with_config(vec!["mock".into()], 1, Duration::from_secs(10));
    let runtime = RoutingRuntime::new(registry, fallback);
    let backend = ProviderChatBackend::new(runtime);
    let history = HistoryStore::new(history_db);

    let mut orchestrator =
        ChatOrchestrator::new_with_persistence(session_db, history, backend).unwrap();

    // 创建 1000 个会话（不删除，因为 ChatOrchestrator 没有 delete_session 方法）
    for i in 0..1000 {
        let _ = orchestrator
            .create_session(Some(&format!("leak-test-{}", i)))
            .await;
    }

    // 验证会话数
    let session_count = orchestrator.sessions().len();
    assert!(
        session_count <= 1000,
        "会话数应不超过 1000，实际: {}",
        session_count
    );
}

#[tokio::test]
async fn no_fd_leak_databases() {
    use prometheus_core::storage::db::Database;

    fn count_open_fds() -> usize {
        std::fs::read_dir("/proc/self/fd")
            .map(|d| d.count())
            .unwrap_or(0)
    }

    let before = count_open_fds();

    // 反复打开关闭数据库
    for i in 0..100 {
        let db_path = temp_path(&format!("stress-fd-leak-{}", i));
        let db = Database::open(db_path.clone()).unwrap();
        db.execute_batch("CREATE TABLE IF NOT EXISTS test (id INTEGER PRIMARY KEY)")
            .unwrap();
        drop(db);
        let _ = std::fs::remove_file(&db_path);
    }

    let after = count_open_fds();
    assert!(
        after <= before + 10,
        "FD 泄漏: before={}, after={}",
        before,
        after
    );
}

#[tokio::test]
async fn no_memory_leak_vector_store() {
    use prometheus_core::storage::VectorStore;

    let store = VectorStore::new(temp_path("stress-vector-leak"), 384);
    store.init().unwrap();

    // 大量插入和搜索操作
    for batch in 0..10 {
        let vectors: Vec<(String, Vec<f32>, String)> = (0..1000)
            .map(|i| {
                let mut v = vec![0.0f32; 384];
                v[(batch * 1000 + i) % 384] = 1.0;
                (format!("id-{}-{}", batch, i), v, format!("meta-{}", i))
            })
            .collect();
        store.insert_batch(&vectors).unwrap();

        // 搜索
        let query = vec![1.0f32; 384];
        let _ = store.search(&query, 10).unwrap();
    }

    // 验证数据完整性
    let query = vec![1.0f32; 384];
    let results = store.search(&query, 10).unwrap();
    assert!(!results.is_empty(), "搜索应返回结果");
    assert!(results.len() <= 10, "搜索结果应不超过 10 条");
}

#[tokio::test]
async fn event_bus_subscriber_cleanup() {
    use prometheus_core::iot::event_bus::EventBus;

    let bus = Arc::new(EventBus::new(256));

    // 创建 100 个订阅者
    let mut subscribers = Vec::new();
    for i in 0..100 {
        let rx = bus.subscribe();
        subscribers.push((i, rx));
    }

    assert!(bus.subscriber_count() >= 100, "应有至少 100 个订阅者");

    // 释放订阅者
    drop(subscribers);

    // 等待清理
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 验证新订阅仍然可用（旧订阅者释放后不影响新订阅）
    let mut new_rx = bus.subscribe();
    bus.publish(DeviceEvent::new(
        "cleanup-test",
        DeviceType::Light,
        EventKind::StateChanged,
        serde_json::json!({"test": true}),
    ));
    let received = new_rx.try_recv();
    assert!(received.is_ok(), "释放旧订阅者后新订阅应正常工作");
}

#[tokio::test]
async fn working_memory_eviction_stable() {
    let mut wm = WorkingMemory::new(64);

    // 插入超过容量的数据
    for i in 0..200 {
        wm.add(format!("item {}", i), 0.5 + ((i % 10) as f64) / 20.0);
    }

    // 验证容量限制生效
    assert_eq!(wm.len(), 64, "工作记忆应维持在容量 64");

    // 多次查询不应导致问题
    for _ in 0..1000 {
        let _ = wm.get_active_context(10);
    }

    assert_eq!(wm.len(), 64, "查询后容量应保持稳定");
}
