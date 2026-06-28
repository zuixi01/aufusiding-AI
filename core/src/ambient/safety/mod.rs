//! 安全子模块
//!
//! 儿童模式、紧急检测、隐私保护、使用统计。

pub mod children_mode;
pub mod content_filter;
pub mod emergency;
pub mod privacy_guard;
pub mod usage_tracker;

#[cfg(test)]
mod tests;

pub use children_mode::ChildrenMode;
pub use emergency::{EmergencyAlert, EmergencyDetector};
pub use privacy_guard::PrivacyGuard;
pub use usage_tracker::UsageTracker;
