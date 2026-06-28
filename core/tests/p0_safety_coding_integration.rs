//! P0 安全分级 + 编程集成 — 跨模块集成测试

use prometheus_core::agent::tools::registry::AgentTool;
use prometheus_core::safety::classification::{DataClassifier, SensitivityLevel};
use prometheus_core::safety::security_mode::{PolicyDecision, SecurityMode, SecurityPolicyEnforcer};

// ── 场景 1: 数据分级驱动安全策略 ──

#[test]
fn classification_drives_security_policy() {
    let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Balanced);

    // 公开文本 → L0 → 允许云端
    let class = DataClassifier::classify("今天天气如何");
    assert_eq!(class.level, SensitivityLevel::Public);
    assert_eq!(enforcer.enforce(&class, true), PolicyDecision::Allow);

    // 含邮箱 → L2 → 强制本地
    let class = DataClassifier::classify("发送到 user@example.com");
    assert_eq!(class.level, SensitivityLevel::Sensitive);
    assert!(matches!(enforcer.enforce(&class, true), PolicyDecision::ForceLocal(_)));

    // 含密码 → L3 → 强制本地
    let class = DataClassifier::classify("我的密码是 hunter2");
    assert_eq!(class.level, SensitivityLevel::Confidential);
    assert!(matches!(enforcer.enforce(&class, true), PolicyDecision::ForceLocal(_)));
}

// ── 场景 2: 偏执模式阻止所有云端路由 ──

#[test]
fn paranoid_blocks_cloud_for_all() {
    let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Paranoid);

    for text in &["你好", "test@mail.com", "密码123", "今天天气"] {
        let class = DataClassifier::classify(text);
        let decision = enforcer.enforce(&class, true);
        assert!(
            matches!(decision, PolicyDecision::ForceLocal(_)),
            "偏执模式应阻止 '{}' 的云端路由",
            text
        );
    }
}

// ── 场景 3: 便捷模式对敏感数据要求匿名化 ──

#[test]
fn convenient_anonymizes_sensitive() {
    let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Convenient);

    // L2 敏感 → 要求匿名化
    let class = DataClassifier::classify("联系 test@example.com");
    assert!(matches!(
        enforcer.enforce(&class, true),
        PolicyDecision::RequireAnonymization(_)
    ));

    // L3 机密 → 阻止
    let class = DataClassifier::classify("api_key=sk-secret123");
    assert!(matches!(
        enforcer.enforce(&class, true),
        PolicyDecision::Block(_)
    ));
}

// ── 场景 4: 编程工具注册到 ToolRegistry ──

#[test]
fn coding_tools_register_to_registry() {
    use prometheus_core::agent::tools::registry::ToolRegistry;
    use prometheus_core::coding::{
        CodeExecutor, ExternalToolDetector, FileOpsTool, ProjectSearchTool,
    };
    use std::path::PathBuf;
    use std::sync::Arc;

    let mut registry = ToolRegistry::new();

    let file_ops = Arc::new(FileOpsTool::new(PathBuf::from("/tmp/sandbox")));
    let search = Arc::new(ProjectSearchTool::new(PathBuf::from("/tmp/project"), 100));
    let detector = Arc::new(ExternalToolDetector::new());
    let executor = Arc::new(CodeExecutor::new(PathBuf::from("/tmp/exec")));

    registry.register(file_ops);
    registry.register(search);
    registry.register(detector);
    registry.register(executor);

    let names = registry.list_names();
    assert!(names.contains(&"file_ops".to_string()));
    assert!(names.contains(&"project_search".to_string()));
    assert!(names.contains(&"tool_detector".to_string()));
    assert!(names.contains(&"code_executor".to_string()));
    assert_eq!(names.len(), 4);
}

// ── 场景 5: FileOpsTool 沙箱边界检查 ──

#[tokio::test]
async fn file_ops_sandbox_boundary() {
    use prometheus_core::coding::FileOpsTool;
    use serde_json::json;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("safe.txt"), "ok").unwrap();

    let tool = FileOpsTool::new(dir.path().to_path_buf());

    // 沙箱内读取 → 成功
    let result = tool
        .execute(json!({"action": "read", "path": "safe.txt"}))
        .await;
    assert!(result.success);

    // 路径遍历 → 失败
    let result = tool
        .execute(json!({"action": "read", "path": "../../../etc/passwd"}))
        .await;
    assert!(!result.success);
}

// ── 场景 6: 端到端分级→策略→路由 ──

#[test]
fn e2e_classification_to_routing() {
    // 模拟完整流程：用户输入 → 分级 → 策略决策
    let enforcer = SecurityPolicyEnforcer::new(SecurityMode::Balanced);

    // 普通编程问题 → L1 → 允许云端
    let class = DataClassifier::classify("帮我写一个快速排序");
    assert_eq!(class.level, SensitivityLevel::Normal);
    assert_eq!(enforcer.enforce(&class, true), PolicyDecision::Allow);
    assert_eq!(enforcer.enforce(&class, false), PolicyDecision::Allow);

    // 含手机号 → L2 → 云端被拒，本地允许
    let class = DataClassifier::classify("给 13800138000 发消息");
    assert_eq!(class.level, SensitivityLevel::Sensitive);
    assert!(matches!(enforcer.enforce(&class, true), PolicyDecision::ForceLocal(_)));
    assert_eq!(enforcer.enforce(&class, false), PolicyDecision::Allow);
}
