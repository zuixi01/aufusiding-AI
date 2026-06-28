//! Daemon Mind 认知架构模块
//!
//! 实现 PBRIC 认知循环、经济皮层、梦境模式、好奇心驱动、思维日志和自我进化。

pub mod belief;
pub mod curiosity;
pub mod dream;
pub mod economics;
pub mod evolution;
pub mod integration;
pub mod pbric;
pub mod predict;
pub mod reflection;
pub mod thought_log;

pub use belief::BeliefModule;
pub use curiosity::CuriosityEngine;
pub use dream::DreamMode;
pub use economics::EconomicCortex;
pub use evolution::EvolutionEngine;
pub use integration::MindIntegration;
pub use pbric::PbricLoop;
pub use predict::PredictModule;
pub use reflection::ReflectionModule;
pub use thought_log::ThoughtLogger;
