//! # Prometheus Core
//!
//! 灵犀 AI 助手核心引擎库。
//! 提供智能路由、对话管理、记忆系统、本地推理、插件运行时等核心能力。

#![allow(clippy::new_without_default)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::len_zero)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::useless_vec)]

pub mod ambient;
pub mod agent;
pub mod channels;
pub mod coding;
pub mod config;
pub mod daemon;
pub mod error;
pub mod inference;
pub mod retry;
pub mod iot;
pub mod logging;
pub mod memory;
pub mod mind;
pub mod orchestrator;
pub mod orchestrator_http;
pub mod persona;
pub mod plugins;
pub mod privacy;
pub mod providers;
pub mod router;
pub mod safety;
pub mod screen;
pub mod security;
pub mod session;
pub mod storage;
pub mod sync;
#[doc(hidden)]
pub mod test_support;
pub mod voice;
pub mod workflow;
