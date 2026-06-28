//! 安全防护模块
//!
//! 提供多层防护机制：
//! - Prompt 注入防护
//! - 命令注入防护
//! - 输入验证
//! - 速率限制
//! - 安全审计日志

pub mod prompt_guard;
pub mod input_validator;
pub mod command_sanitizer;
pub mod rate_limiter;
pub mod audit_log;
pub mod errors;

pub use prompt_guard::PromptGuard;
pub use input_validator::InputValidator;
pub use command_sanitizer::CommandSanitizer;
pub use rate_limiter::RateLimiter;
pub use audit_log::{SecurityAuditLog, SecurityEvent, SecurityEventType, Severity};
pub use errors::{SecurityError, ValidationError};
