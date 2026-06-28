//! 质量门禁模块
//!
//! Phase 5: 三道门禁（功能/安全/架构）、审查结果、打回机制

pub mod gate;
pub mod review;

pub use gate::{GateKind, GateOutcome, GatePipeline};
pub use review::{
    RejectionAction, RejectionRecord, ReviewCategory, ReviewComment, ReviewResult,
};
