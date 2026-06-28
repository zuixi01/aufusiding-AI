//! 集成模块 — Agent 集群与外部系统的桥梁
//!
//! Phase 7: 接入 ChatOrchestrator，端到端测试

pub mod agent_orchestrator;
pub mod bridge;
pub mod complexity;

pub use agent_orchestrator::{AgentOrchestrator, ClusterResult};
pub use complexity::{ComplexityAssessor, TaskRoute};
