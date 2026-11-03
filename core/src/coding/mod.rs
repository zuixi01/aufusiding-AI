//! 编程集成模块
//!
//! 提供文件操作、代码搜索、外部工具检测与运行、代码执行、沙箱隔离等编程能力。
//! 所有工具实现 AgentTool trait，可注册到 Agent 工具系统。

pub mod detector;
pub mod executor;
pub mod file_ops;
pub mod runner;
pub mod sandbox;
pub mod search;

pub use detector::ExternalToolDetector;
pub use executor::CodeExecutor;
pub use file_ops::FileOpsTool;
pub use runner::ExternalToolRunner;
pub use sandbox::{CodeSandbox, ResourceLimits, SandboxConfig, SandboxPermissions};
pub use search::ProjectSearchTool;
