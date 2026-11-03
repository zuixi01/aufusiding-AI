//! Agent 集群模块
//!
//! Phase 1: 基础框架 — 类型系统、身份注入、Agent 运行时生命周期
//! Phase 2: 通信系统 — 消息总线、直接消息、群组讨论
//! Phase 3: 大管家 — 任务拆解、团队组建、进度监控、升级策略
//! Phase 4: 执行引擎 — 模型连接池、上下文管理、ReAct 循环、工具系统
//! Phase 5: 质量门禁 — 三道门禁、审查结果、打回机制
//! Phase 6: 共识机制 — 圆桌会议、加权投票、辩论流程、冲突解决
//! Phase 7: 集成联调 — 接入 ChatOrchestrator、端到端流程

pub mod comm;
pub mod consensus;
pub mod error;
pub mod execution;
pub mod identity;
pub mod integration;
pub mod quality;
pub mod runtime;
pub mod steward;
pub mod tools;
pub mod types;

pub use comm::{AgentBus, AgentEnvelope, AgentMessage, DirectMailbox, GroupManager};
pub use consensus::{Roundtable, VoteResult, WeightedVote};
pub use error::AgentError;
pub use execution::{AgentContext, AgentExecutor, ExecutionOutcome, ModelPool};
pub use identity::{AgentIdentity, IdentityInjector, IdentityVault};
pub use integration::{AgentOrchestrator, ClusterResult, ComplexityAssessor, TaskRoute};
pub use quality::{GateOutcome, GatePipeline, ReviewResult};
pub use runtime::{AgentRuntime, LiveAgent};
pub use steward::{Steward, TaskGraph, TaskGraphBuilder};
pub use tools::ToolRegistry;
pub use types::*;
