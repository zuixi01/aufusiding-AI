//! 流式语音类型定义
//!
//! 音频分块、流事件、VAD 判定、管线配置。

use serde::{Deserialize, Serialize};

/// 音频流分块
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// PCM 样本
    pub samples: Vec<f32>,
    /// 采样率 (Hz)
    pub sample_rate: u32,
    /// 声道数
    pub channels: u16,
    /// 单调递增序号
    pub sequence_id: u64,
    /// 相对流起始的时间偏移 (ms)
    pub timestamp_ms: u64,
}

impl AudioChunk {
    /// 创建新的音频块
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16, seq: u64, ts: u64) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
            sequence_id: seq,
            timestamp_ms: ts,
        }
    }

    /// 块时长 (ms)
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0;
        }
        let frames = self.samples.len() as u64 / self.channels as u64;
        frames * 1000 / self.sample_rate as u64
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// 流式管线事件
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 部分转录结果
    PartialTranscript { text: String, is_final: bool },
    /// 合成的音频块
    SynthesizedChunk(AudioChunk),
    /// VAD 检测到语音开始
    SpeechStarted,
    /// VAD 检测到语音结束
    SpeechEnded,
    /// 错误
    Error(String),
    /// 流结束
    Done,
}

/// VAD 判定结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VadDecision {
    /// 语音
    Speech,
    /// 静音
    Silence,
}

/// 流式管线配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    /// 采样率 (Hz)
    pub sample_rate: u32,
    /// 声道数
    pub channels: u16,
    /// 每块时长 (ms)
    pub chunk_duration_ms: u32,
    /// VAD 能量阈值
    pub vad_threshold: f32,
    /// 静音超时 (ms)，超过后结束当前轮次
    pub silence_timeout_ms: u64,
    /// 最大轮次时长 (ms)
    pub max_turn_duration_ms: u64,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            chunk_duration_ms: 30,
            vad_threshold: 0.5,
            silence_timeout_ms: 1000,
            max_turn_duration_ms: 30000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_chunk_duration() {
        let chunk = AudioChunk::new(vec![0.0; 480], 16000, 1, 0, 0);
        assert_eq!(chunk.duration_ms(), 30); // 480 / 16000 * 1000 = 30ms
    }

    #[test]
    fn audio_chunk_stereo_duration() {
        let chunk = AudioChunk::new(vec![0.0; 960], 16000, 2, 0, 0);
        assert_eq!(chunk.duration_ms(), 30); // 960 / 2 / 16000 * 1000 = 30ms
    }

    #[test]
    fn audio_chunk_empty() {
        let chunk = AudioChunk::new(vec![], 16000, 1, 0, 0);
        assert!(chunk.is_empty());
        assert_eq!(chunk.duration_ms(), 0);
    }

    #[test]
    fn streaming_config_defaults() {
        let cfg = StreamingConfig::default();
        assert_eq!(cfg.sample_rate, 16000);
        assert_eq!(cfg.channels, 1);
        assert_eq!(cfg.chunk_duration_ms, 30);
        assert_eq!(cfg.silence_timeout_ms, 1000);
    }

    #[test]
    fn vad_decision_equality() {
        assert_eq!(VadDecision::Speech, VadDecision::Speech);
        assert_ne!(VadDecision::Speech, VadDecision::Silence);
    }

    #[test]
    fn audio_chunk_zero_sample_rate() {
        let chunk = AudioChunk::new(vec![0.0; 100], 0, 1, 0, 0);
        assert_eq!(chunk.duration_ms(), 0);
    }
}
