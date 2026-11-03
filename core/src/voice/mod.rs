//! 语音引擎模块
//!
//! 集成 Whisper STT、Piper TTS 和唤醒词检测，实现完整的语音交互。
//! 包含流式实时语音管线和 VAD。

pub mod http_tts;
pub mod http_wakeword;
pub mod mock;
pub mod pipeline;
pub mod streaming_http_stt;
pub mod streaming_http_tts;
pub mod streaming_mock;
pub mod streaming_pipeline;
pub mod streaming_traits;
pub mod streaming_types;
pub mod stt;
pub mod traits;
pub mod tts;
pub mod types;
pub mod vad;
pub mod wakeword;
pub mod whisper_stt;

pub use pipeline::VoicePipeline;
pub use streaming_pipeline::StreamingVoicePipeline;
pub use streaming_traits::{StreamingSttEngine, StreamingTtsEngine, VadEngine};
pub use streaming_types::{AudioChunk, StreamEvent, StreamingConfig, VadDecision};
pub use stt::SpeechToText;
pub use traits::{SpeechToTextEngine, TextToSpeechEngine, WakewordEngine};
pub use tts::TextToSpeech;
pub use types::AudioBuffer;
pub use vad::EnergyVad;
pub use wakeword::WakewordDetector;
