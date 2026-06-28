//! 消息渠道模块
//!
//! 支持多种消息渠道（Telegram、Slack、微信等）的统一接口。

pub mod slack;
pub mod telegram;
pub mod traits;
pub mod wechat;

pub use traits::{MessageChannel, MockMessageChannel};
