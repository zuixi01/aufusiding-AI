//! 身份模块 — 身份注入系统

pub mod competency;
pub mod injection;
pub mod roles;
pub mod vault;

pub use injection::AgentIdentity;
pub use injection::IdentityInjector;
pub use vault::IdentityVault;
