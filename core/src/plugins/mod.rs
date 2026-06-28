//! 插件运行时模块
//!
//! 插件管理、注册、沙箱权限控制。

pub mod capabilities;
pub mod capability_bridge;
pub mod capability_registry;
pub mod manager;
pub mod registry;
pub mod runtime;
pub mod sandbox;
pub mod wasm_executor;

pub use manager::{MockPluginExecutor, PluginExecutor, PluginManager};
pub use registry::PluginRegistry;
pub use sandbox::Sandbox;
