//! P4 跨模块集成测试

use prometheus_core::iot::registry::DeviceRegistry;
use prometheus_core::iot::{
    DeviceController, DeviceDiscovery, MockDeviceController, SecurePairing,
};
use prometheus_core::memory::profile::UserProfile;
use prometheus_core::mind::dream::{DreamMode, DreamTask};
use prometheus_core::persona::adaptive::AdaptiveLearner;
use prometheus_core::persona::{PersonaEngine, PersonaTemplate};
use prometheus_core::screen::accessibility::AccessAction;
use prometheus_core::screen::types::{ElementType, ScreenRegion, UIElement};
use prometheus_core::screen::{
    MockAccessibilityBridge, MockScreenAnalyzer, MockScreenCapture, ScreenAgent,
};
use prometheus_core::sync::crypto::E2EEncryption;
use prometheus_core::sync::engine::SyncEngine;
use prometheus_core::sync::protocol::{Operation, SyncProtocol};
use prometheus_core::sync::transport::create_transport_pair;
use prometheus_core::sync::Transport;
use std::collections::HashMap;
use uuid::Uuid;

/// 1. IoT 完整流程: 发现 → 配对 → 控制 → 状态查询 → 解除配对
#[tokio::test]
async fn iot_full_lifecycle() {
    // 发现
    let mut discovery = DeviceDiscovery::new();
    let devices = discovery.scan(1).await.unwrap();
    assert!(!devices.is_empty());
    let device_id = &devices[0].id;

    // 配对
    let mut pairing = SecurePairing::new();
    let pin = pairing.initiate(device_id).await.unwrap();
    pairing.confirm(&pin).await.unwrap();
    assert!(pairing.is_paired(device_id));

    // 控制
    let ctrl = MockDeviceController::new("Light");
    ctrl.register_device(device_id, "Light");
    ctrl.turn_on(device_id).await.unwrap();
    let state = ctrl.get_state(device_id).await.unwrap();
    assert_eq!(state["powered"], true);

    // 解除配对
    pairing.unpair(device_id).await.unwrap();
    assert!(!pairing.is_paired(device_id));
}

/// 1b. IoT 注册表持久化
#[test]
fn iot_registry_persistence() {
    let path = format!("/tmp/p4-integ-registry-{}.db", Uuid::new_v4());
    let reg = DeviceRegistry::new(&path).unwrap();
    let dev = prometheus_core::iot::registry::RegisteredDevice {
        id: "light-001".into(),
        name: "客厅灯".into(),
        device_type: "Light".into(),
        ip: "192.168.1.10".into(),
        port: 8080,
        protocol: "Matter".into(),
        paired: false,
        paired_at: None,
    };
    reg.register(&dev).unwrap();
    reg.mark_paired("light-001").unwrap();
    let loaded = reg.get("light-001").unwrap().unwrap();
    assert!(loaded.paired);
}

/// 2. 同步完整流程: 创建操作 → 加密 → 传输 → 解密 → 合并 → 验证一致性
#[tokio::test]
async fn sync_full_flow() {
    // 两个设备各自创建操作
    let mut proto_a = SyncProtocol::with_index("dev-a".into(), 0, 2);
    let mut proto_b = SyncProtocol::with_index("dev-b".into(), 1, 2);

    let id1 = Uuid::new_v4();
    let op_a = proto_a.create_op(Operation::Insert {
        id: id1,
        data: "from-a".into(),
    });

    let id2 = Uuid::new_v4();
    let op_b = proto_b.create_op(Operation::Insert {
        id: id2,
        data: "from-b".into(),
    });

    // 加密
    let enc = E2EEncryption::generate_keypair().unwrap();
    let serialized_a = serde_json::to_vec(&op_a).unwrap();
    let encrypted = enc.encrypt(&serialized_a, enc.public_key()).unwrap();
    let decrypted = enc.decrypt(&encrypted).unwrap();
    assert_eq!(serialized_a, decrypted);

    // 签名验证
    let sig = enc.sign(&serialized_a).unwrap();
    assert!(enc.verify(&serialized_a, &sig, enc.public_key()).unwrap());

    // 合并
    let merged = proto_a.merge(&[op_a], std::slice::from_ref(&op_b));
    assert_eq!(merged.len(), 2);

    // 应用远程操作
    proto_a.apply_op(&op_b);
    assert_eq!(proto_a.state().len(), 2);
    assert_eq!(proto_a.state().get(&id1).unwrap(), "from-a");
    assert_eq!(proto_a.state().get(&id2).unwrap(), "from-b");
}

/// 2b. 同步引擎 + 传输
#[tokio::test]
async fn sync_engine_with_transport() {
    let enc = E2EEncryption::generate_keypair().unwrap();
    let mut engine = SyncEngine::new("dev-1".into(), enc);
    engine.push_change(Operation::Insert {
        id: Uuid::new_v4(),
        data: "test".into(),
    });
    assert_eq!(engine.get_status().pending_ops, 1);

    let (mut a, _b) = create_transport_pair();
    a.connect("mock://").await.unwrap();
    engine.sync(&a).await.unwrap();
    assert_eq!(engine.get_status().pending_ops, 0);
    assert!(engine.get_status().last_sync.is_some());
}

/// 3. 屏幕代理: 捕获 → 分析 → 查找元素 → 执行动作
#[tokio::test]
async fn screen_agent_flow() {
    let elements = vec![
        UIElement {
            id: "btn-submit".into(),
            element_type: ElementType::Button,
            label: "提交".into(),
            bounds: ScreenRegion {
                x: 100,
                y: 200,
                width: 80,
                height: 30,
            },
            children: vec![],
            properties: HashMap::new(),
        },
        UIElement {
            id: "input-name".into(),
            element_type: ElementType::TextField,
            label: "姓名".into(),
            bounds: ScreenRegion {
                x: 10,
                y: 10,
                width: 200,
                height: 30,
            },
            children: vec![],
            properties: HashMap::new(),
        },
    ];

    let mut agent = ScreenAgent::new(
        Box::new(MockScreenCapture::new(1920, 1080)),
        Box::new(MockScreenAnalyzer::new()),
        Box::new(MockAccessibilityBridge::new(elements)),
    );

    // 捕获上下文
    let ctx = agent.capture_context().await.unwrap();
    assert_eq!(ctx.app_name, "MockApp");
    assert_eq!(ctx.screen_size, (1920, 1080));

    // 查找元素
    let found = agent.find_element("提交").await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "btn-submit");

    // 执行动作
    agent
        .perform_action("btn-submit", AccessAction::Click)
        .await
        .unwrap();

    // 历史记录
    assert_eq!(agent.history_len(), 1);

    // 摘要
    let summary = agent.get_context_summary();
    assert!(summary.contains("MockApp"));
}

/// 4. 个性化: 用户画像更新 → 人格自动选择 → 主动回忆 → 梦境整理
#[tokio::test]
async fn personalization_flow() {
    // 用户画像
    let mut profile = UserProfile::new("user-1");
    profile
        .update_from_conversation(&[
            "I love Rust and Python programming".into(),
            "Interested in AI and 机器学习".into(),
        ])
        .await
        .unwrap();
    assert!(profile.interests.contains(&"rust".to_string()));
    assert!(profile.interests.contains(&"ai".to_string()));

    // 人格引擎 + 自动选择
    let mut engine = PersonaEngine::new();
    let tech = PersonaTemplate::tech_expert();
    let tech_id = tech.id;
    engine.load_persona(tech);
    engine.load_persona(PersonaTemplate::creative_writer());
    let selected = engine.auto_select("帮我编程和架构设计");
    assert_eq!(selected, Some(tech_id));
    engine.switch_persona(tech_id).unwrap();
    assert!(engine.get_system_prompt().unwrap().contains("技术专家"));

    // 自适应学习
    let mut learner = AdaptiveLearner::new();
    learner.record_feedback("i1", 0.9);
    learner.record_feedback("i2", 0.8);
    let style = learner.get_style_adjustments();
    assert!(style.verbosity > 0.0);

    // 梦境模式
    let mut dream = DreamMode::new();
    assert!(dream.should_dream());
    dream.enter().await.unwrap();
    let reports = dream.run_all().await.unwrap();
    assert_eq!(reports.len(), 4);
    assert!(reports
        .iter()
        .any(|r| r.task == DreamTask::MemoryConsolidation));
    dream.exit();
    assert!(!dream.is_dreaming());
}

/// 5. 跨模块: IoT + Persona + Profile 联动
#[tokio::test]
async fn cross_module_integration() {
    // 发现设备
    let mut discovery = DeviceDiscovery::new();
    let devices = discovery.scan(1).await.unwrap();
    assert!(!devices.is_empty());

    // 根据设备类型选择人格
    let mut persona_engine = PersonaEngine::new();
    persona_engine.load_persona(PersonaTemplate::life_assistant());
    persona_engine.load_persona(PersonaTemplate::tech_expert());
    // 生活助手应该处理 IoT 相关的"日程管理"和"生活建议"
    let selected = persona_engine.auto_select("帮我日程管理和生活建议");
    assert!(selected.is_some());

    // 用户画像记录 IoT 使用偏好
    let mut profile = UserProfile::new("user-1");
    profile
        .preferences
        .insert("iot_enabled".into(), "true".into());
    profile.interests.push("智能家居".into());

    // 画像相似度
    let mut other = UserProfile::new("user-2");
    other.interests.push("智能家居".into());
    let sim = profile.similarity(&other);
    assert!(sim > 0.0);
}
