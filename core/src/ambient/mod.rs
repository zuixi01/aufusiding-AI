//! 环境计算模块
//!
//! 24h 常驻语音循环 — VAD → 唤醒词 → STT → 意图分发 → 执行 → TTS 回复。
//! 实现 Jarvis 范式的环境计算体验。

pub mod audio_sink;
pub mod audio_source;
pub mod bargein;
pub mod biometrics;
pub mod config;
pub mod conversation;
#[cfg(feature = "audio-capture")]
pub mod cpal_audio_sink;
#[cfg(feature = "audio-capture")]
pub mod cpal_audio_source;
pub mod daemon;
pub mod daemon_builder;
pub mod dispatcher_trait;
pub mod intent;
pub mod loop_runner;
pub mod mode_manager;
pub mod privacy_dispatcher;
pub mod proactive;
pub mod response;
pub mod room_context;
pub mod safety;
pub mod suggestion_announcer;
pub mod system_audio_sink;
pub mod system_audio_source;
pub mod types;
pub mod wakeword_enhanced;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_e2e;

pub use audio_sink::{AudioSink, MockAudioSink};
pub use audio_source::{AudioSource, MockAudioSource};
pub use config::AmbientConfig;
pub use daemon::AmbientDaemon;
pub use intent::{DispatchResult, DispatchTarget, IntentDispatcher};
pub use loop_runner::AmbientLoopRunner;
pub use mode_manager::{
    MockModeTransitionHook, ModeManager, ModeProfile, ModeTransition, ModeTransitionHook,
    NotificationPolicy, SystemMode,
};
pub use response::ResponseFormatter;
pub use room_context::RoomContext;
pub use system_audio_sink::{AudioSinkConfig, SystemAudioSink};
pub use system_audio_source::{AudioSourceConfig, SystemAudioSource};
pub use types::{AmbientState, ConversationTurn, ListeningMode};
pub use wakeword_enhanced::EnhancedWakewordDetector;
