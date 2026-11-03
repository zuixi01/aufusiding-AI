//! 环境计算配置
//!
//! 唤醒词、置信度阈值、房间、设备 ID、本地处理开关等。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 环境计算配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientConfig {
    /// 唤醒词文本（如 "小普"）
    #[serde(default = "default_wakeword")]
    pub wakeword: String,
    /// 唤醒词模型路径
    #[serde(default)]
    pub wakeword_model_path: PathBuf,
    /// 唤醒词声学置信度阈值（0.0-1.0）
    #[serde(default = "default_wakeword_confidence")]
    pub wakeword_confidence_threshold: f32,
    /// 唤醒后等待后续语音的超时（毫秒）
    #[serde(default = "default_post_wakeword_timeout_ms")]
    pub post_wakeword_timeout_ms: u64,
    /// 静音超时（毫秒），超过后结束当前监听
    #[serde(default = "default_silence_timeout_ms")]
    pub silence_timeout_ms: u64,
    /// 连续对话模式超时（毫秒）
    #[serde(default = "default_conversation_timeout_ms")]
    pub conversation_timeout_ms: u64,
    /// 最大连续对话轮数
    #[serde(default = "default_max_conversation_turns")]
    pub max_conversation_turns: usize,
    /// 默认房间名称
    #[serde(default = "default_room")]
    pub default_room: String,
    /// 设备 ID（本机标识）
    #[serde(default = "default_device_id")]
    pub device_id: String,
    /// 是否强制本地处理（不发送到云端）
    #[serde(default)]
    pub local_only: bool,
    /// 采样率
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    /// 声道数
    #[serde(default = "default_channels")]
    pub channels: u16,
    /// 是否启用声纹验证（Phase 2）
    #[serde(default)]
    pub voiceprint_enabled: bool,
}

fn default_wakeword() -> String {
    "小普".into()
}
fn default_wakeword_confidence() -> f32 {
    0.85
}
fn default_post_wakeword_timeout_ms() -> u64 {
    3000
}
fn default_silence_timeout_ms() -> u64 {
    1500
}
fn default_conversation_timeout_ms() -> u64 {
    30000
}
fn default_max_conversation_turns() -> usize {
    10
}
fn default_room() -> String {
    "default".into()
}
fn default_device_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn default_sample_rate() -> u32 {
    16000
}
fn default_channels() -> u16 {
    1
}

impl Default for AmbientConfig {
    fn default() -> Self {
        Self {
            wakeword: default_wakeword(),
            wakeword_model_path: PathBuf::new(),
            wakeword_confidence_threshold: default_wakeword_confidence(),
            post_wakeword_timeout_ms: default_post_wakeword_timeout_ms(),
            silence_timeout_ms: default_silence_timeout_ms(),
            conversation_timeout_ms: default_conversation_timeout_ms(),
            max_conversation_turns: default_max_conversation_turns(),
            default_room: default_room(),
            device_id: default_device_id(),
            local_only: false,
            sample_rate: default_sample_rate(),
            channels: default_channels(),
            voiceprint_enabled: false,
        }
    }
}
