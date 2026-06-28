//! Agent 执行引擎模块
//!
//! Phase 4: 模型连接池、上下文管理、ReAct 执行循环

pub mod cascade;
pub mod context;
pub mod executor;
pub mod model_pool;
pub mod semantic_cache;
pub mod termination;

pub use context::AgentContext;
pub use executor::{AgentExecutor, ExecutionOutcome, ExecutorConfig};
pub use model_pool::{ModelBackend, ModelHandle, ModelPool, ModelTier};
