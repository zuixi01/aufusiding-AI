//! 安全防护模块
//!
//! 提供多层防护机制：
//! - Prompt 注入防护
//! - 命令注入防护
//! - 输入验证
//! - 速率限制
//! - 安全审计日志

pub mod audit_log;
pub mod command_sanitizer;
pub mod errors;
pub mod input_validator;
pub mod prompt_guard;
pub mod rate_limiter;

pub use audit_log::{SecurityAuditLog, SecurityEvent, SecurityEventType, Severity};
pub use command_sanitizer::CommandSanitizer;
pub use errors::{SecurityError, ValidationError};
pub use input_validator::InputValidator;
pub use prompt_guard::PromptGuard;
pub use rate_limiter::RateLimiter;
