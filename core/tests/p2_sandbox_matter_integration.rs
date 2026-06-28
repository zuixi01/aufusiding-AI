//! P2 代码沙箱 + OS沙箱 + Matter协议 — 跨模块集成测试

use prometheus_core::coding::sandbox::{
    CodeSandbox, ResourceLimits, SandboxConfig, SandboxPermissions,
};
use prometheus_core::iot::adapters::matter::{
    MatterAdapter, MatterDeviceType,
};
use prometheus_core::iot::control::DeviceController;
use prometheus_core::safety::os_sandbox::{
    LandlockRuleset, SandboxCapability,
    SandboxEnvironment, SandboxPolicy, SeccompProfile,
};
use std::path::PathBuf;

// ── 场景 1: 代码沙箱执行 + 环境隔离 ──

#[tokio::test]
async fn sandbox_executes_code_with_clean_env() {
    let dir = tempfile::tempdir().unwrap();
    let config = SandboxConfig::new(dir.path().to_path_buf())
        .with_env("SANDBOX_TEST", "isolated");
    let sandbox = CodeSandbox::new(config);

    let result = sandbox.execute_code("bash", "echo $SANDBOX_TEST").await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.trim() == "isolated");
}

// ── 场景 2: 沙箱资源限制 — 输出截断 ──

#[tokio::test]
async fn sandbox_truncates_large_output() {
    let dir = tempfile::tempdir().unwrap();
    let config = SandboxConfig::new(dir.path().to_path_buf()).with_limits(ResourceLimits {
        max_output_bytes: 50,
        ..ResourceLimits::default()
    });
    let sandbox = CodeSandbox::new(config);

    let result = sandbox
        .execute_code("bash", "for i in $(seq 1 1000); do echo line$i; done")
        .await
        .unwrap();
    assert!(result.stdout.len() <= 80); // 50 + truncation message
}

// ── 场景 3: 沙箱路径验证 ──

#[test]
fn sandbox_path_validation() {
    let dir = tempfile::tempdir().unwrap();
    let test_file = dir.path().join("data.txt");
    std::fs::write(&test_file, "test").unwrap();

    let sandbox = CodeSandbox::new(
        SandboxConfig::new(dir.path().to_path_buf()).with_permissions(SandboxPermissions {
            fs_read: true,
            fs_write: true,
            writable_paths: vec![dir.path().to_path_buf()],
            ..SandboxPermissions::default()
        }),
    );

    assert!(sandbox.validate_path(&test_file, false)); // 读取允许
    assert!(sandbox.validate_path(&test_file, true)); // 写入允许（在白名单内）
}

// ── 场景 4: OS 沙箱 Landlock 规则构建 ──

#[test]
fn landlock_rules_from_policy() {
    let policy = SandboxPolicy {
        read_paths: vec![PathBuf::from("/usr"), PathBuf::from("/lib")],
        write_paths: vec![PathBuf::from("/tmp/sandbox")],
        exec_paths: vec![PathBuf::from("/usr/bin")],
        allow_network: false,
        allow_fork: false,
    };

    let ruleset = LandlockRuleset::from_policy(&policy);
    assert_eq!(ruleset.rules().len(), 4);

    // 验证访问控制
    assert!(ruleset.check_access(std::path::Path::new("/usr/lib/x.so"), false));
    assert!(!ruleset.check_access(std::path::Path::new("/usr/lib/x.so"), true));
    assert!(ruleset.check_access(std::path::Path::new("/tmp/sandbox/out"), true));
    assert!(!ruleset.check_access(std::path::Path::new("/etc/shadow"), false));
}

// ── 场景 5: Seccomp 配置文件 ──

#[test]
fn seccomp_profiles_work() {
    let minimal = SeccompProfile::minimal();
    assert!(minimal.is_allowed("read"));
    assert!(minimal.is_allowed("write"));
    assert!(!minimal.is_allowed("execve"));
    assert!(!minimal.is_allowed("socket"));

    let permissive = SeccompProfile::permissive();
    assert!(permissive.is_allowed("anything"));
}

// ── 场景 6: OS 沙箱平台检测 ──

#[test]
fn os_sandbox_detects_capabilities() {
    let env = SandboxEnvironment::detect();
    // 所有平台至少支持进程隔离
    assert!(env.has_capability(SandboxCapability::ProcessIsolation));

    if cfg!(target_os = "linux") {
        assert!(env.has_capability(SandboxCapability::SyscallFiltering));
        assert!(env.has_capability(SandboxCapability::FilesystemIsolation));
    }
}

// ── 场景 7: Matter 适配器注册 + DeviceController trait ──

#[test]
fn matter_adapter_registers_devices() {
    let mut adapter = MatterAdapter::new("http://matter.local:5540", 1);
    adapter.register_node("light-1", 100, "客厅灯", MatterDeviceType::OnOffLight);
    adapter.register_node("thermo-1", 101, "温控器", MatterDeviceType::Thermostat);

    assert_eq!(adapter.supported_type(), "matter");
    assert_eq!(adapter.fabric_id(), 1);
}

// ── 场景 8: Matter 设备类型映射 ──

#[test]
fn matter_device_type_mapping() {
    let types = vec![
        (0x0100, MatterDeviceType::OnOffLight),
        (0x0101, MatterDeviceType::DimmableLight),
        (0x0301, MatterDeviceType::Thermostat),
        (0x000A, MatterDeviceType::DoorLock),
    ];
    for (id, expected) in types {
        assert_eq!(MatterDeviceType::from_id(id), expected);
    }
}
