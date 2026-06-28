//! 自动化工作流模块
//!
//! 支持用户自定义的自动化工作流，包括触发器和动作。

pub mod actions;
pub mod context;
pub mod engine;
pub mod event_bridge;
pub mod scheduler;
pub mod step;
pub mod triggers;

pub use actions::{Action, ActionExecutionContext};
pub use context::WorkflowContext;
pub use engine::{Workflow, WorkflowEngine, WorkflowExecutionReport, WorkflowExecutionStatus};
pub use scheduler::WorkflowScheduler;
pub use step::{RetryPolicy, Step, StepExecutionRecord, StepStatus, WorkflowStep};
pub use triggers::Trigger;
