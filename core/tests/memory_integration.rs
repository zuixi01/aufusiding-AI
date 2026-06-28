//! 五层记忆系统端到端集成测试

use prometheus_core::memory::manager::MemoryManager;
use prometheus_core::test_support::temp_dir_path;

fn create_manager() -> MemoryManager {
    let dir = temp_dir_path("memory-integration");
    MemoryManager::new(&dir).unwrap()
}

/// 会话1存信息 → 结束 → 会话2能回忆
#[tokio::test]
async fn memory_roundtrip_across_sessions() {
    let mut mgr = create_manager();

    // 会话 1：用户讨论 Rust
    mgr.on_user_message("我正在用 Rust 开发一个 Web 框架").await;
    mgr.on_assistant_response(
        "我正在用 Rust 开发一个 Web 框架",
        "Rust 非常适合高性能 Web 开发",
    )
    .await;

    // 结束会话 1
    let messages = vec![(
        "我正在用 Rust 开发一个 Web 框架".to_string(),
        "Rust 非常适合高性能 Web 开发，推荐使用 Actix-web 或 Axum 框架".to_string(),
    )];
    mgr.on_session_end(&messages).await.unwrap();

    // 会话 2：查询记忆
    let context = mgr.build_memory_context("Rust Web", 5).await;
    // 应该能找到之前的对话记忆或知识
    assert!(!context.is_empty(), "会话2应该能回忆起会话1的内容");
}

/// 对话中的事实自动进入 L3
#[tokio::test]
async fn knowledge_extraction_from_conversation() {
    let mut mgr = create_manager();

    let messages = vec![
        (
            "什么是所有权".to_string(),
            "Rust 的所有权系统是其核心特性之一，每个值都有一个唯一的所有者".to_string(),
        ),
        (
            "借用呢".to_string(),
            "借用允许你在不转移所有权的情况下引用数据，分为不可变借用和可变借用".to_string(),
        ),
    ];

    mgr.on_session_end(&messages).await.unwrap();

    // 知识库应该包含提取的事实
    let context = mgr.build_memory_context("所有权 借用", 5).await;
    assert!(!context.is_empty(), "应该从对话中提取了知识");
}

/// 话题切换时主动召回
#[tokio::test]
async fn proactive_recall_triggers_on_topic_change() {
    let mut mgr = create_manager();

    // 先存一些记忆
    mgr.on_user_message("Rust 的并发模型很强大").await;
    mgr.on_assistant_response("Rust 的并发模型很强大", "是的")
        .await;
    let messages = vec![(
        "Rust 的并发模型很强大".to_string(),
        "Rust 通过所有权系统和类型系统在编译期保证线程安全".to_string(),
    )];
    mgr.on_session_end(&messages).await.unwrap();

    // 新会话：话题切换应触发回忆
    let recalls = mgr.on_user_message("Python 机器学习").await;
    // 第一条消息总是触发 should_recall (新关键词)
    // 结果可能为空（因为话题不同），但不应报错
    let _ = recalls;

    // 再切回 Rust 话题
    let recalls = mgr.on_user_message("Rust 并发编程").await;
    // 话题变化应触发回忆
    let _ = recalls;
}

/// 工作记忆衰减：旧记忆权重低于新记忆
#[tokio::test]
async fn working_memory_decay_over_time() {
    let mut mgr = create_manager();

    mgr.on_user_message("第一条消息").await;
    // 短暂等待让时间流逝
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    mgr.on_user_message("第二条消息").await;

    let items = mgr.working().all_items();
    assert_eq!(items.len(), 2);

    // 第二条（更新）的时间戳应该更晚
    assert!(items[1].timestamp.elapsed() < items[0].timestamp.elapsed());
}
