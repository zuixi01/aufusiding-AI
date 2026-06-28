//! 人格引擎模块
//!
//! 支持多人格切换，每个人格有独立的语气、知识和行为模式。

pub mod adaptive;
pub mod engine;
pub mod templates;

pub use adaptive::AdaptiveLearner;
pub use engine::{Persona, PersonaEngine, PromptContextBuilder};
pub use templates::PersonaTemplate;
