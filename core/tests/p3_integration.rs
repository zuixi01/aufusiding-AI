//! P3 跨模块集成测试

use chrono::Utc;
use prometheus_core::memory::episodic::EpisodicMemory;
use prometheus_core::memory::knowledge::KnowledgeBase;
use prometheus_core::memory::working::WorkingMemory;
use prometheus_core::plugins::manager::PluginManager;
use prometheus_core::plugins::sandbox::Permissions;
use prometheus_core::privacy::anonymization::Anonymizer;
use prometheus_core::privacy::consent::{ConsentManager, ConsentPurpose};
use prometheus_core::privacy::deletion::DataEraser;
use prometheus_core::privacy::export::DataExporter;
use prometheus_core::session::history::{HistoryStore, Message, Role};
use prometheus_core::voice::mock::{MockSTT, MockTTS, MockWakeword};
use prometheus_core::voice::pipeline::VoicePipeline;
use prometheus_core::voice::types::AudioBuffer;
use std::path::PathBuf;
use uuid::Uuid;

fn temp_path(prefix: &str) -> PathBuf {
    prometheus_core::test_support::temp_db_path(&format!("p3-integ-{prefix}"))
}

// WAT add module bytecode
const ADD_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01,
    0x7F, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, 0x0A, 0x09,
    0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B,
];

// ── 场景 1: WASM 插件通过 PluginManager 执行 + 沙箱权限 ──

#[test]
fn wasm_plugin_via_manager_with_sandbox() {
    let mut mgr = PluginManager::new(prometheus_core::test_support::temp_dir_path("plugins"));

    // 有权限 → 加载成功
    let perms = Permissions {
        wasm_execute: true,
        ..Default::default()
    };
    mgr.load_wasm("adder", "WASM 加法", ADD_WASM, perms)
        .unwrap();
    assert!(mgr.is_loaded("adder"));

    let result = mgr
        .execute("adder", r#"{"function":"add","args":[100,200]}"#)
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["result"], 300);

    // 无权限 → 加载失败
    let no_perms = Permissions::default();
    assert!(mgr
        .load_wasm("blocked", "desc", ADD_WASM, no_perms)
        .is_err());
    assert!(!mgr.is_loaded("blocked"));

    // 卸载
    mgr.unload("adder").unwrap();
    assert!(!mgr.is_loaded("adder"));
}

// ── 场景 2: 语音管线 MockSTT → 文本处理 → MockTTS ──

#[test]
fn voice_pipeline_full_flow() {
    let stt = Box::new(MockSTT::new("你好世界"));
    let tts = Box::new(MockTTS::new(16000, 1));
    let wakeword = Box::new(MockWakeword::new(true));

    let pipeline = VoicePipeline::new(stt, tts).with_wakeword(wakeword);

    let audio_in = AudioBuffer::new(vec![0.1; 16000], 16000, 1).unwrap();

    // 完整管线: 唤醒词检测 → STT → 文本处理 → TTS
    let audio_out = pipeline
        .process(&audio_in, |text| format!("回复: {}", text))
        .unwrap();

    assert!(!audio_out.is_empty());
    assert_eq!(audio_out.sample_rate, 16000);
    assert_eq!(audio_out.channels, 1);

    // 唤醒词未检测到 → 失败
    let pipeline_no_wake = VoicePipeline::new(
        Box::new(MockSTT::new("test")),
        Box::new(MockTTS::new(16000, 1)),
    )
    .with_wakeword(Box::new(MockWakeword::new(false)));

    assert!(pipeline_no_wake
        .process(&audio_in, |t| t.to_string())
        .is_err());
}

// ── 场景 3: 隐私完整流程 ──

#[tokio::test]
async fn privacy_full_flow() {
    // 1. 记录同意
    let consent_mgr = ConsentManager::new(temp_path("consent")).unwrap();
    consent_mgr
        .record_consent("user1", ConsentPurpose::DataCollection, true)
        .unwrap();
    consent_mgr
        .record_consent("user1", ConsentPurpose::Analytics, true)
        .unwrap();
    assert!(consent_mgr.check_consent("user1", ConsentPurpose::DataCollection));

    // 2. 准备数据
    let history = HistoryStore::new(temp_path("hist"));
    let session_id = Uuid::new_v4();
    for i in 0..3 {
        history
            .append(&Message {
                id: Uuid::new_v4(),
                session_id,
                role: Role::User,
                content: format!("消息 {}", i),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
    }

    let episodic = EpisodicMemory::new(temp_path("ep"), 128).unwrap();
    let knowledge = KnowledgeBase::new(temp_path("kb"), 128).unwrap();
    let mut working = WorkingMemory::new(64);
    working.add("工作记忆条目".into(), 0.8);

    // 3. 导出数据
    let exporter = DataExporter::new(&history, &episodic, &knowledge, &working);
    let export = exporter.export_all(session_id).await.unwrap();
    assert_eq!(export["history"]["count"], 3);
    assert_eq!(export["working_memory"]["count"], 1);

    // 4. 匿名化
    let text = "用户邮箱 test@example.com 手机 13912345678";
    let anonymized = Anonymizer::anonymize(text);
    assert!(anonymized.contains("[EMAIL]"));
    assert!(anonymized.contains("[PHONE]"));
    assert!(!anonymized.contains("test@example.com"));

    // 5. 删除数据
    let mut episodic_mut = EpisodicMemory::new(temp_path("ep2"), 128).unwrap();
    let mut working_mut = WorkingMemory::new(64);
    working_mut.add("to delete".into(), 0.5);

    let history2 = HistoryStore::new(temp_path("hist2"));
    let sid2 = Uuid::new_v4();
    history2
        .append(&Message {
            id: Uuid::new_v4(),
            session_id: sid2,
            role: Role::User,
            content: "delete me".into(),
            timestamp: Utc::now(),
        })
        .await
        .unwrap();

    let mut eraser = DataEraser::new(&history2, &mut episodic_mut, &mut working_mut);
    let receipt = eraser.delete_all(sid2).await.unwrap();
    assert_eq!(receipt.messages_deleted, 1);
    assert!(receipt.working_cleared);

    // 验证删除后数据不可恢复
    let remaining = history2.get_session_messages(sid2).await.unwrap();
    assert!(remaining.is_empty());

    // 6. 撤销同意
    consent_mgr
        .revoke_consent("user1", ConsentPurpose::DataCollection)
        .unwrap();
    assert!(!consent_mgr.check_consent("user1", ConsentPurpose::DataCollection));
}

// ── 场景 4: WASM 插件处理匿名化后的文本 ──

#[test]
fn wasm_plugin_with_anonymized_input() {
    let mut mgr = PluginManager::new(prometheus_core::test_support::temp_dir_path("plugins"));
    let perms = Permissions {
        wasm_execute: true,
        ..Default::default()
    };
    mgr.load_wasm("adder", "加法", ADD_WASM, perms).unwrap();

    // 先匿名化文本
    let raw = "用户 test@mail.com 请求计算 3+5";
    let anonymized = Anonymizer::anonymize(raw);
    assert!(anonymized.contains("[EMAIL]"));

    // 用 WASM 插件执行计算
    let result = mgr
        .execute("adder", r#"{"function":"add","args":[3,5]}"#)
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["result"], 8);
}
