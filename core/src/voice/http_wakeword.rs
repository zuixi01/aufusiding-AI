//! 本地唤醒词检测引擎
//!
//! 基于能量阈值的关键词检测骨架。纯本地计算，无需 HTTP。

use super::traits::WakewordEngine;
use super::types::AudioBuffer;

/// 本地唤醒词配置
#[derive(Debug, Clone)]
pub struct LocalWakewordConfig {
    /// 唤醒关键词
    pub keyword: String,
    /// 能量阈值（RMS 高于此值视为有语音活动）
    pub energy_threshold: f32,
}

impl Default for LocalWakewordConfig {
    fn default() -> Self {
        Self {
            keyword: "hey prometheus".to_string(),
            energy_threshold: 0.08,
        }
    }
}

/// 本地唤醒词检测引擎
///
/// 使用能量检测作为第一级过滤：只有当音频能量超过阈值时
/// 才认为可能包含唤醒词。这是一个骨架实现，真实场景下
/// 应集成 Porcupine 或类似的关键词检测库。
pub struct LocalWakewordEngine {
    config: LocalWakewordConfig,
}

impl LocalWakewordEngine {
    /// 创建本地唤醒词引擎
    pub fn new(config: LocalWakewordConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self {
            config: LocalWakewordConfig::default(),
        }
    }

    /// 获取关键词
    pub fn keyword(&self) -> &str {
        &self.config.keyword
    }

    /// 计算 RMS 能量
    fn rms_energy(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }
}

impl WakewordEngine for LocalWakewordEngine {
    fn detect(&self, audio: &AudioBuffer) -> anyhow::Result<bool> {
        if audio.is_empty() {
            return Ok(false);
        }
        let energy = Self::rms_energy(&audio.samples);
        // 能量检测作为第一级过滤
        // 真实实现应在此处运行关键词模型
        Ok(energy > self.config.energy_threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loud_audio() -> AudioBuffer {
        let samples: Vec<f32> = (0..16000).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        AudioBuffer::new(samples, 16000, 1).unwrap()
    }

    #[test]
    fn detect_returns_true_for_loud_audio() {
        let engine = LocalWakewordEngine::with_defaults();
        assert!(engine.detect(&loud_audio()).unwrap());
    }

    #[test]
    fn detect_returns_false_for_silence() {
        let engine = LocalWakewordEngine::with_defaults();
        let silence = AudioBuffer::silence(1.0, 16000, 1);
        assert!(!engine.detect(&silence).unwrap());
    }

    #[test]
    fn detect_returns_false_for_empty_audio() {
        let engine = LocalWakewordEngine::with_defaults();
        let empty = AudioBuffer::new(vec![], 16000, 1).unwrap();
        assert!(!engine.detect(&empty).unwrap());
    }
}
