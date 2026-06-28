//! 对话管理模块
//!
//! 管理多轮对话的会话生命周期、上下文窗口压缩和历史记录持久化。

pub mod context;
pub mod history;
pub mod manager;

pub use context::ContextCompressor;
pub use history::HistoryStore;
pub use manager::{Session, SessionManager, SessionState};
