//! 语音生物识别子模块
//!
//! 声纹识别、说话人档案、注册流程。

pub mod enrollment;
pub mod speaker_profile;
pub mod speaker_store;
pub mod voiceprint;

#[cfg(test)]
mod tests;

pub use enrollment::EnrollmentFlow;
pub use speaker_profile::{PermissionLevel, SpeakerProfile};
pub use speaker_store::SpeakerStore;
pub use voiceprint::VoiceprintEngine;
