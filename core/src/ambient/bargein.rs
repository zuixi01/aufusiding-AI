//! 打断检测
//!
//! TTS 播放中检测到用户语音 → 立即停止播放。

use crate::voice::streaming_traits::VadEngine;
use crate::voice::streaming_types::{AudioChunk, VadDecision};

/// 打断检测器
pub struct BargeInDetector {
    /// 连续语音超过此时长视为打断（毫秒）
    speech_threshold_ms: u64,
    /// 当前连续语音时长（毫秒）
    accumulated_speech_ms: u64,
}

impl BargeInDetector {
    pub fn new(speech_threshold_ms: u64) -> Self {
        Self {
            speech_threshold_ms,
            accumulated_speech_ms: 0,
        }
    }

    /// 检测是否发生打断
    ///
    /// - `vad`: VAD 引擎
    /// - `chunk`: 当前音频块
    /// - `is_speaking`: 系统是否正在 TTS 播放
    pub fn check(
        &mut self,
        vad: &dyn VadEngine,
        chunk: &AudioChunk,
        is_speaking: bool,
    ) -> bool {
        if !is_speaking {
            self.accumulated_speech_ms = 0;
            return false;
        }

        match vad.classify(chunk) {
            Ok(VadDecision::Speech) => {
                self.accumulated_speech_ms += chunk.duration_ms();
                self.accumulated_speech_ms >= self.speech_threshold_ms
            }
            _ => {
                self.accumulated_speech_ms = 0;
                false
            }
        }
    }

    /// 重置累积计数
    pub fn reset(&mut self) {
        self.accumulated_speech_ms = 0;
    }

    /// 获取打断阈值
    pub fn threshold_ms(&self) -> u64 {
        self.speech_threshold_ms
    }
}
