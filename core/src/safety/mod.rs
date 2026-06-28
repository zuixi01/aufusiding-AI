//! 安全系统模块
//!
//! 包含安全金库、权限体系、智能扫描入库、危机感知、数据分级、安全模式和 OS 原生沙箱。

pub mod classification;
pub mod crisis;
pub mod os_sandbox;
pub mod permissions;
pub mod scanner;
pub mod security_mode;
pub mod vault;

pub use classification::{DataClassification, DataClassifier, SensitivityLevel};
pub use crisis::CrisisDetector;
pub use os_sandbox::{SandboxEnvironment, SandboxPolicy, SandboxPlatform, LandlockRuleset, SeccompProfile};
pub use permissions::PermissionManager;
pub use scanner::SmartScanner;
pub use security_mode::{PolicyDecision, SecurityMode, SecurityPolicyEnforcer};
pub use vault::SecureVault;
