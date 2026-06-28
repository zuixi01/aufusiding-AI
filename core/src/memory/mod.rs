//! 记忆系统模块
//!
//! 五层记忆架构：L0 用户画像 → L1 工作记忆 → L2 情景记忆 → L3 知识库 → L4 主动回忆

pub mod embedding;
pub mod episodic;
pub mod knowledge;
pub mod manager;
pub mod proactive;
pub mod profile;
pub mod working;

pub use episodic::EpisodicMemory;
pub use knowledge::KnowledgeBase;
pub use manager::MemoryManager;
pub use proactive::ProactiveRecall;
pub use profile::UserProfile;
pub use working::WorkingMemory;
