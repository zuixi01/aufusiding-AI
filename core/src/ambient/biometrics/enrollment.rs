//! 声纹注册流程
//!
//! 语音引导录制多段样本，提取声纹并注册。

use crate::voice::AudioBuffer;

use super::speaker_profile::{PermissionLevel, SpeakerProfile};
use super::voiceprint::VoiceprintEngine;

/// 注册流程状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentState {
    /// 等待开始
    Pending,
    /// 录制中（当前第 N 段）
    Recording(usize),
    /// 处理中
    Processing,
    /// 完成
    Completed,
    /// 失败
    Failed,
}

/// 声纹注册流程
pub struct EnrollmentFlow {
    engine: VoiceprintEngine,
    /// 需要录制的样本数
    required_samples: usize,
    /// 已录制的音频样本
    samples: Vec<AudioBuffer>,
    /// 当前状态
    state: EnrollmentState,
}

impl EnrollmentFlow {
    pub fn new(engine: VoiceprintEngine, required_samples: usize) -> Self {
        Self {
            engine,
            required_samples,
            samples: Vec::new(),
            state: EnrollmentState::Pending,
        }
    }

    /// 获取当前状态
    pub fn state(&self) -> EnrollmentState {
        self.state
    }

    /// 需要的样本数
    pub fn required_samples(&self) -> usize {
        self.required_samples
    }

    /// 已录制的样本数
    pub fn collected_samples(&self) -> usize {
        self.samples.len()
    }

    /// 获取当前步骤的语音提示
    pub fn prompt(&self) -> String {
        match self.state {
            EnrollmentState::Pending => "请开始录制您的声音样本。".into(),
            EnrollmentState::Recording(n) => {
                format!(
                    "请说一段话，这是第 {} 段，共需 {} 段。",
                    n + 1,
                    self.required_samples
                )
            }
            EnrollmentState::Processing => "正在处理您的声纹，请稍候...".into(),
            EnrollmentState::Completed => "声纹注册完成！".into(),
            EnrollmentState::Failed => "声纹注册失败，请重试。".into(),
        }
    }

    /// 开始注册
    pub fn start(&mut self) {
        self.samples.clear();
        self.state = EnrollmentState::Recording(0);
    }

    /// 提交一段音频样本
    pub fn submit_sample(&mut self, audio: AudioBuffer) -> anyhow::Result<()> {
        if !matches!(self.state, EnrollmentState::Recording(_)) {
            anyhow::bail!("当前状态不允许提交样本");
        }
        if audio.duration_secs() < 1.0 {
            anyhow::bail!("音频样本太短，至少需要 1 秒");
        }

        self.samples.push(audio);
        let count = self.samples.len();

        if count >= self.required_samples {
            self.state = EnrollmentState::Processing;
        } else {
            self.state = EnrollmentState::Recording(count);
        }
        Ok(())
    }

    /// 完成注册，生成说话人档案
    pub fn finalize(
        &mut self,
        name: String,
        permission_level: PermissionLevel,
    ) -> anyhow::Result<SpeakerProfile> {
        if !matches!(self.state, EnrollmentState::Processing) {
            anyhow::bail!("样本尚未收集完成");
        }

        // 从所有样本提取 embedding 并取平均
        let mut embeddings = Vec::new();
        for sample in &self.samples {
            match self.engine.extract_embedding(sample) {
                Ok(emb) => embeddings.push(emb),
                Err(e) => {
                    self.state = EnrollmentState::Failed;
                    return Err(e);
                }
            }
        }

        if embeddings.is_empty() {
            self.state = EnrollmentState::Failed;
            anyhow::bail!("无法提取声纹特征");
        }

        let dim = embeddings[0].len();
        let mut avg = vec![0.0f32; dim];
        for emb in &embeddings {
            for (i, v) in emb.iter().enumerate() {
                avg[i] += v;
            }
        }
        let count = embeddings.len() as f32;
        for v in &mut avg {
            *v /= count;
        }

        // L2 归一化
        let norm: f32 = avg.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut avg {
                *v /= norm;
            }
        }

        self.state = EnrollmentState::Completed;
        Ok(SpeakerProfile::new(name, avg, permission_level))
    }

    /// 重置流程
    pub fn reset(&mut self) {
        self.samples.clear();
        self.state = EnrollmentState::Pending;
    }
}
