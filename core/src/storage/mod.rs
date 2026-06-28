//! 存储层模块
//!
//! 基于 SQLite + SQLCipher 的加密存储，集成 sqlite-vec 向量搜索。

pub mod db;
pub mod encryption;
pub mod usage;
pub mod vector;

pub use db::Database;
pub use encryption::EncryptionManager;
pub use usage::UsageStore;
pub use vector::VectorStore;
