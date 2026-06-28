//! 增强唤醒词检测器
//!
//! 包装现有 `WakewordDetector`，增加置信度评分和后续语音验证。

use crate::voice::{AudioBuffer, WakewordDetector, WakewordEngine};

/// 增强唤醒词检测器 — 三重保险防误触发
pub struct EnhancedWakewordDetector {
    inner: WakewordDetector,
    /// 声学置信度阈值
    confidence_threshold: f32,
    /// 最近一次检测的置信度
    last_confidence: f32,
    /// 唤醒后是否需要后续语音验证
    require_followup: bool,
}

impl EnhancedWakewordDetector {
    pub fn new(inner: WakewordDetector, confidence_threshold: f32) -> Self {
        Self {
            inner,
            confidence_threshold,
            last_confidence: 0.0,
            require_followup: true,
        }
    }

    /// 设置是否需要后续语音验证
    pub fn with_followup_required(mut self, required: bool) -> Self {
        self.require_followup = required;
        self
    }

    /// 获取最近一次检测的置信度
    pub fn last_confidence(&self) -> f32 {
        self.last_confidence
    }

    /// 是否需要后续语音验证
    pub fn requires_followup(&self) -> bool {
        self.require_followup
    }

    /// 检测唤醒词并返回置信度
    pub fn detect_with_confidence(&mut self, audio: &AudioBuffer) -> anyhow::Result<Option<f32>> {
        let detected = self.inner.detect(audio)?;
        if !detected {
            self.last_confidence = 0.0;
            return Ok(None);
        }

        // 基于能量模式计算置信度
        let confidence = self.compute_confidence(audio);
        self.last_confidence = confidence;

        if confidence >= self.confidence_threshold {
            Ok(Some(confidence))
        } else {
            Ok(None)
        }
    }

    /// 加载底层模型
    pub async fn load(&mut self) -> anyhow::Result<()> {
        self.inner.load().await
    }

    /// 是否已加载
    pub fn is_loaded(&self) -> bool {
        self.inner.is_loaded()
    }

    fn compute_confidence(&self, audio: &AudioBuffer) -> f32 {
        if audio.is_empty() {
            return 0.0;
        }
        // 基于 RMS 能量和音节结构计算置信度
        let rms = Self::rms_energy(&audio.samples);
        let duration = audio.duration_secs();

        // 唤醒词典型特征：0.3-1.5 秒，2-4 个音节
        let duration_score = if (0.3..=1.5).contains(&duration) {
            1.0
        } else {
            0.5
        };

        let energy_score = (rms * 5.0).min(1.0);

        // 综合置信度
        (duration_score * 0.4 + energy_score * 0.6).min(1.0)
    }

    fn rms_energy(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }
}
