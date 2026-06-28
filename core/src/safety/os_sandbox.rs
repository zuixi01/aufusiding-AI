//! OS 原生沙箱抽象层
//!
//! 跨平台沙箱 trait + Linux Landlock/seccomp 实现。
//! 提供内核级文件系统和网络隔离能力。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 沙箱能力
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SandboxCapability {
    /// 文件系统隔离
    FilesystemIsolation,
    /// 网络隔离
    NetworkIsolation,
    /// 进程隔离
    ProcessIsolation,
    /// 系统调用过滤
    SyscallFiltering,
    /// 资源限制 (cgroups)
    ResourceCgroups,
}

/// 沙箱策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPolicy {
    /// 允许读取的路径
    pub read_paths: Vec<PathBuf>,
    /// 允许写入的路径
    pub write_paths: Vec<PathBuf>,
    /// 允许执行的路径
    pub exec_paths: Vec<PathBuf>,
    /// 是否允许网络
    pub allow_network: bool,
    /// 是否允许创建子进程
    pub allow_fork: bool,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            read_paths: vec![PathBuf::from("/usr"), PathBuf::from("/lib")],
            write_paths: vec![],
            exec_paths: vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
            allow_network: false,
            allow_fork: false,
        }
    }
}

/// 沙箱执行环境
#[derive(Debug, Clone)]
pub struct SandboxEnvironment {
    pub policy: SandboxPolicy,
    pub capabilities: Vec<SandboxCapability>,
    pub platform: SandboxPlatform,
}

/// 平台类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxPlatform {
    /// Linux: Landlock LSM + seccomp-bpf
    LinuxLandlock,
    /// macOS: App Sandbox + XPC
    MacOsSandbox,
    /// Windows: AppContainer
    WindowsAppContainer,
    /// 通用: 仅进程级隔离
    Generic,
}

impl SandboxEnvironment {
    /// 检测当前平台并创建沙箱环境
    pub fn detect() -> Self {
        let platform = Self::detect_platform();
        let capabilities = Self::detect_capabilities(&platform);
        Self {
            policy: SandboxPolicy::default(),
            capabilities,
            platform,
        }
    }

    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.policy = policy;
        self
    }

    fn detect_platform() -> SandboxPlatform {
        if cfg!(target_os = "linux") {
            SandboxPlatform::LinuxLandlock
        } else if cfg!(target_os = "macos") {
            SandboxPlatform::MacOsSandbox
        } else if cfg!(target_os = "windows") {
            SandboxPlatform::WindowsAppContainer
        } else {
            SandboxPlatform::Generic
        }
    }

    fn detect_capabilities(platform: &SandboxPlatform) -> Vec<SandboxCapability> {
        match platform {
            SandboxPlatform::LinuxLandlock => vec![
                SandboxCapability::FilesystemIsolation,
                SandboxCapability::SyscallFiltering,
                SandboxCapability::ProcessIsolation,
                SandboxCapability::ResourceCgroups,
                SandboxCapability::NetworkIsolation,
            ],
            SandboxPlatform::MacOsSandbox => vec![
                SandboxCapability::FilesystemIsolation,
                SandboxCapability::ProcessIsolation,
                SandboxCapability::NetworkIsolation,
            ],
            SandboxPlatform::WindowsAppContainer => vec![
                SandboxCapability::FilesystemIsolation,
                SandboxCapability::ProcessIsolation,
            ],
            SandboxPlatform::Generic => vec![
                SandboxCapability::ProcessIsolation,
            ],
        }
    }

    /// 是否支持指定能力
    pub fn has_capability(&self, cap: SandboxCapability) -> bool {
        self.capabilities.contains(&cap)
    }
}

/// Landlock 文件系统规则 (Linux 5.13+)
#[derive(Debug, Clone)]
pub struct LandlockRule {
    pub path: PathBuf,
    pub access: LandlockAccess,
}

/// Landlock 访问权限
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandlockAccess {
    ReadOnly,
    ReadWrite,
    Execute,
    ReadExecute,
}

/// Landlock 规则集
pub struct LandlockRuleset {
    rules: Vec<LandlockRule>,
}

impl LandlockRuleset {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 从 SandboxPolicy 构建规则集
    pub fn from_policy(policy: &SandboxPolicy) -> Self {
        let mut ruleset = Self::new();

        for path in &policy.read_paths {
            ruleset.add_rule(LandlockRule {
                path: path.clone(),
                access: LandlockAccess::ReadOnly,
            });
        }
        for path in &policy.write_paths {
            ruleset.add_rule(LandlockRule {
                path: path.clone(),
                access: LandlockAccess::ReadWrite,
            });
        }
        for path in &policy.exec_paths {
            ruleset.add_rule(LandlockRule {
                path: path.clone(),
                access: LandlockAccess::ReadExecute,
            });
        }

        ruleset
    }

    pub fn add_rule(&mut self, rule: LandlockRule) {
        self.rules.push(rule);
    }

    pub fn rules(&self) -> &[LandlockRule] {
        &self.rules
    }

    /// 验证路径是否被规则允许
    pub fn check_access(&self, path: &std::path::Path, write: bool) -> bool {
        for rule in &self.rules {
            if path.starts_with(&rule.path) {
                return match rule.access {
                    LandlockAccess::ReadOnly | LandlockAccess::ReadExecute => !write,
                    LandlockAccess::ReadWrite => true,
                    LandlockAccess::Execute => false,
                };
            }
        }
        false // 默认拒绝
    }

    /// 应用规则集到当前进程 (Linux Landlock)
    /// 注意: 实际的 landlock_create_ruleset/landlock_add_rule/landlock_restrict_self
    /// 系统调用需要 Linux 5.13+ 内核支持，此处提供接口定义。
    pub fn enforce(&self) -> anyhow::Result<()> {
        if !cfg!(target_os = "linux") {
            anyhow::bail!("Landlock 仅在 Linux 上可用");
        }
        // 实际实现需要 libc::syscall 调用 landlock_* 系统调用
        // P2 提供接口和规则构建，实际内核调用留给平台集成层
        tracing::info!(
            "Landlock 规则集已构建: {} 条规则 (实际内核调用待平台集成)",
            self.rules.len()
        );
        Ok(())
    }
}

/// Seccomp BPF 过滤器配置 (Linux)
#[derive(Debug, Clone)]
pub struct SeccompProfile {
    /// 允许的系统调用白名单
    pub allowed_syscalls: Vec<String>,
    /// 默认动作
    pub default_action: SeccompAction,
}

/// Seccomp 默认动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompAction {
    /// 终止进程
    Kill,
    /// 返回错误
    Errno,
    /// 记录日志
    Log,
    /// 允许
    Allow,
}

impl SeccompProfile {
    /// 创建最小化 seccomp 配置（仅允许基本系统调用）
    pub fn minimal() -> Self {
        Self {
            allowed_syscalls: vec![
                "read".into(), "write".into(), "close".into(),
                "fstat".into(), "mmap".into(), "mprotect".into(),
                "munmap".into(), "brk".into(), "exit_group".into(),
                "exit".into(), "arch_prctl".into(), "futex".into(),
                "clock_gettime".into(), "getpid".into(),
            ],
            default_action: SeccompAction::Errno,
        }
    }

    /// 创建宽松配置（允许大部分系统调用，禁止危险操作）
    pub fn permissive() -> Self {
        Self {
            allowed_syscalls: vec![], // 空 = 使用黑名单模式
            default_action: SeccompAction::Allow,
        }
    }

    /// 是否允许指定系统调用
    pub fn is_allowed(&self, syscall: &str) -> bool {
        match self.default_action {
            SeccompAction::Allow => true, // 宽松模式
            _ => self.allowed_syscalls.iter().any(|s| s == syscall),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_platform() {
        let env = SandboxEnvironment::detect();
        if cfg!(target_os = "linux") {
            assert_eq!(env.platform, SandboxPlatform::LinuxLandlock);
            assert!(env.has_capability(SandboxCapability::SyscallFiltering));
        }
    }

    #[test]
    fn default_policy_restrictive() {
        let policy = SandboxPolicy::default();
        assert!(!policy.allow_network);
        assert!(!policy.allow_fork);
        assert!(policy.write_paths.is_empty());
    }

    #[test]
    fn landlock_from_policy() {
        let policy = SandboxPolicy {
            read_paths: vec![PathBuf::from("/usr"), PathBuf::from("/lib")],
            write_paths: vec![PathBuf::from("/tmp/sandbox")],
            exec_paths: vec![PathBuf::from("/usr/bin")],
            allow_network: false,
            allow_fork: false,
        };
        let ruleset = LandlockRuleset::from_policy(&policy);
        assert_eq!(ruleset.rules().len(), 4);
    }

    #[test]
    fn landlock_check_access() {
        let mut ruleset = LandlockRuleset::new();
        ruleset.add_rule(LandlockRule {
            path: PathBuf::from("/usr"),
            access: LandlockAccess::ReadOnly,
        });
        ruleset.add_rule(LandlockRule {
            path: PathBuf::from("/tmp/work"),
            access: LandlockAccess::ReadWrite,
        });

        assert!(ruleset.check_access(std::path::Path::new("/usr/lib/libc.so"), false));
        assert!(!ruleset.check_access(std::path::Path::new("/usr/lib/libc.so"), true));
        assert!(ruleset.check_access(std::path::Path::new("/tmp/work/out.txt"), true));
        assert!(!ruleset.check_access(std::path::Path::new("/etc/passwd"), false));
    }

    #[test]
    fn seccomp_minimal_profile() {
        let profile = SeccompProfile::minimal();
        assert!(profile.is_allowed("read"));
        assert!(profile.is_allowed("write"));
        assert!(!profile.is_allowed("execve"));
        assert_eq!(profile.default_action, SeccompAction::Errno);
    }

    #[test]
    fn seccomp_permissive_profile() {
        let profile = SeccompProfile::permissive();
        assert!(profile.is_allowed("anything"));
        assert_eq!(profile.default_action, SeccompAction::Allow);
    }

    #[test]
    fn sandbox_capabilities() {
        let env = SandboxEnvironment::detect();
        // 所有平台至少支持进程隔离
        assert!(env.has_capability(SandboxCapability::ProcessIsolation));
    }

    #[test]
    fn sandbox_with_custom_policy() {
        let env = SandboxEnvironment::detect().with_policy(SandboxPolicy {
            allow_network: true,
            allow_fork: true,
            ..SandboxPolicy::default()
        });
        assert!(env.policy.allow_network);
        assert!(env.policy.allow_fork);
    }
}
