//! 智能路由器模块
//!
//! 根据用户意图、任务复杂度和资源状况，智能选择本地推理或云端 API。

pub mod capability;
pub mod catalog;
pub mod classifier;
pub mod cost;
pub mod difficulty;
pub mod fallback;
pub mod runtime;
pub mod steward;
pub mod strategy;
pub mod token_estimator;

pub use capability::{CapabilityMatrix, ModelId};
pub use catalog::{CapabilityDomain, ModelCatalog, ModelTier};
pub use classifier::IntentClassifier;
pub use cost::CostCalculator;
pub use difficulty::{assess_difficulty, TaskDifficulty};
pub use steward::LocalSteward;
pub use strategy::{Router, RoutingStrategy};
