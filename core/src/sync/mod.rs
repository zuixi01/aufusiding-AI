//! 多设备同步模块
//!
//! 基于 CRDT 的冲突解决 + E2E 加密的多设备数据同步。

pub mod crypto;
pub mod engine;
pub mod protocol;
pub mod transport;

pub use crypto::E2EEncryption;
pub use engine::SyncEngine;
pub use protocol::SyncProtocol;
pub use transport::Transport;
