//! 大管家模块 — Agent 集群的核心编排系统
//!
//! Phase 3: 任务拆解、团队组建、进度监控、升级策略

pub mod dedup;
pub mod escalation;
pub mod monitor;
pub mod orchestrator;
pub mod planner;
pub mod team_builder;

pub use dedup::TaskDeduplicator;
pub use escalation::{EscalationAction, EscalationPolicy};
pub use monitor::ProgressMonitor;
pub use orchestrator::{OrchestrationSession, Steward};
pub use planner::{SubTask, SubTaskStatus, TaskGraph, TaskGraphBuilder};
pub use team_builder::TeamBuilder;
