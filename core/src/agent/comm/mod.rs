//! Agent 内部通信系统
//!
//! Phase 2: 消息总线、直接消息、群组讨论、消息协议

pub mod blackboard;
pub mod bus;
pub mod direct;
pub mod group;
pub mod protocol;

pub use blackboard::Blackboard;
pub use bus::AgentBus;
pub use direct::DirectMailbox;
pub use group::GroupManager;
pub use protocol::{
    AgentEnvelope, AgentMessage, EscalationReason, MessageFilter, MessageTarget, ReviewComment,
    Severity, Stance,
};
