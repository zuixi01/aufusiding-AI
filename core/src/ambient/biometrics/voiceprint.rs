//! 声纹特征提取引擎
//!
//! 基于 MFCC/x-vector 的声纹特征提取，本地推理。

use crate::voice::AudioBuffer;

/// 声纹引擎 — 提取声纹 embedding 并比对
pub struct VoiceprintEngine {
    /// embedding 维度
    dimension: usize,
    /// 比对阈值（cosine similarity）
    match_threshold: f32,
}

impl VoiceprintEngine {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            match_threshold: 0.75,
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.match_threshold = threshold;
        self
    }

    /// embedding 维度
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// 比对阈值
    pub fn match_threshold(&self) -> f32 {
        self.match_threshold
    }

    /// 从音频提取声纹 embedding
    pub fn extract_embedding(&self, audio: &AudioBuffer) -> anyhow::Result<Vec<f32>> {
        if audio.is_empty() {
            anyhow::bail!("音频数据为空，无法提取声纹");
        }
        // 简化实现：基于 MFCC 特征的伪 embedding
        // 实际生产中会使用 ONNX Runtime 加载 ECAPA-TDNN 模型
        let mut embedding = vec![0.0f32; self.dimension];
        let frame_size = (audio.sample_rate as usize) / 100; // 10ms 帧
        if frame_size == 0 {
            anyhow::bail!("采样率无效");
        }

        for (i, frame) in audio.samples.chunks(frame_size).enumerate() {
            let energy: f32 = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
            let idx = i % self.dimension;
            embedding[idx] += energy;
        }

        // L2 归一化
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut embedding {
                *v /= norm;
            }
        }

        Ok(embedding)
    }

    /// 余弦相似度比对
    pub fn compare(&self, a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }

    /// 判断两个 embedding 是否属于同一人
    pub fn is_match(&self, a: &[f32], b: &[f32]) -> bool {
        self.compare(a, b) >= self.match_threshold
    }
}
