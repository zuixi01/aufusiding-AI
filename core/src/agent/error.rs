//! Agent 集群错误类型

use super::types::AgentId;

/// Agent 模块专用错误
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("agent not found: {0}")]
    NotFound(AgentId),

    #[error("role not found in vault: {0}")]
    RoleNotFound(String),

    #[error("agent already exists: {0}")]
    AlreadyExists(AgentId),

    #[error("agent is in invalid state: {0}")]
    InvalidState(String),

    #[error("identity injection failed: {0}")]
    InjectionFailed(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
