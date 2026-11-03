//! 语音活动检测 (VAD)
//!
//! 基于 RMS 能量的 VAD 实现，无外部依赖。

use super::streaming_traits::VadEngine;
use super::streaming_types::{AudioChunk, VadDecision};

/// 基于能量的 VAD
pub struct EnergyVad {
    threshold: f32,
}

impl EnergyVad {
    /// 创建 VAD，threshold 为 RMS 能量阈值 (0.0-1.0)
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold: threshold.clamp(0.0, 1.0),
        }
    }

    /// 计算 RMS 能量
    fn rms_energy(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    /// 获取阈值
    pub fn threshold(&self) -> f32 {
        self.threshold
    }
}

impl VadEngine for EnergyVad {
    fn classify(&self, chunk: &AudioChunk) -> anyhow::Result<VadDecision> {
        let energy = Self::rms_energy(&chunk.samples);
        if energy > self.threshold {
            Ok(VadDecision::Speech)
        } else {
            Ok(VadDecision::Silence)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silence_chunk() -> AudioChunk {
        AudioChunk::new(vec![0.0; 480], 16000, 1, 0, 0)
    }

    fn loud_chunk() -> AudioChunk {
        AudioChunk::new(vec![0.8; 480], 16000, 1, 0, 0)
    }

    #[test]
    fn silence_detected() {
        let vad = EnergyVad::new(0.1);
        assert_eq!(
            vad.classify(&silence_chunk()).unwrap(),
            VadDecision::Silence
        );
    }

    #[test]
    fn speech_detected() {
        let vad = EnergyVad::new(0.1);
        assert_eq!(vad.classify(&loud_chunk()).unwrap(), VadDecision::Speech);
    }

    #[test]
    fn threshold_boundary() {
        // RMS of [0.5; 480] = 0.5
        let chunk = AudioChunk::new(vec![0.5; 480], 16000, 1, 0, 0);
        let vad_low = EnergyVad::new(0.49);
        let vad_high = EnergyVad::new(0.51);
        assert_eq!(vad_low.classify(&chunk).unwrap(), VadDecision::Speech);
        assert_eq!(vad_high.classify(&chunk).unwrap(), VadDecision::Silence);
    }

    #[test]
    fn empty_samples_is_silence() {
        let vad = EnergyVad::new(0.01);
        let chunk = AudioChunk::new(vec![], 16000, 1, 0, 0);
        assert_eq!(vad.classify(&chunk).unwrap(), VadDecision::Silence);
    }

    #[test]
    fn threshold_clamped() {
        let vad = EnergyVad::new(2.0);
        assert_eq!(vad.threshold(), 1.0);
        let vad2 = EnergyVad::new(-1.0);
        assert_eq!(vad2.threshold(), 0.0);
    }
}
