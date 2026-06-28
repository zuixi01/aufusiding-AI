//! 语音转文字 (STT)
//!
//! 本地 STT 引擎实现。提供基于能量检测的简易语音活动分析，
//! 以及可扩展的 Whisper 集成点。

use super::traits::SpeechToTextEngine;
use super::types::AudioBuffer;

/// 语音转文字引擎
pub struct SpeechToText {
    /// Whisper 模型路径
    model_path: std::path::PathBuf,
    /// 语言代码
    language: String,
    /// 是否已加载模型
    loaded: bool,
    /// 静音阈值（RMS 低于此值视为静音）
    silence_threshold: f32,
}

impl SpeechToText {
    /// 创建 STT 引擎
    pub fn new(model_path: std::path::PathBuf, language: &str) -> Self {
        Self {
            model_path,
            language: language.to_string(),
            loaded: false,
            silence_threshold: 0.01,
        }
    }

    /// 设置静音阈值
    pub fn with_silence_threshold(mut self, threshold: f32) -> Self {
        self.silence_threshold = threshold;
        self
    }

    /// 加载模型（当前为本地分析模式，无需外部模型）
    pub async fn load(&mut self) -> anyhow::Result<()> {
        if !self.model_path.as_os_str().is_empty() && self.model_path.exists() {
            // 未来集成 whisper.cpp 时在此加载模型
        }
        self.loaded = true;
        Ok(())
    }

    /// 是否已加载
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// 获取语言
    pub fn language(&self) -> &str {
        &self.language
    }

    /// 计算 RMS 能量
    fn rms_energy(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }

    /// 检测语音段（返回有语音活动的帧区间）
    fn detect_speech_segments(&self, audio: &AudioBuffer) -> Vec<(usize, usize)> {
        let frame_size = (audio.sample_rate as usize * audio.channels as usize) / 50; // 20ms 帧
        if frame_size == 0 {
            return vec![];
        }
        let mut segments = Vec::new();
        let mut in_speech = false;
        let mut start = 0;

        for (i, chunk) in audio.samples.chunks(frame_size).enumerate() {
            let energy = Self::rms_energy(chunk);
            if energy > self.silence_threshold {
                if !in_speech {
                    start = i * frame_size;
                    in_speech = true;
                }
            } else if in_speech {
                segments.push((start, i * frame_size));
                in_speech = false;
            }
        }
        if in_speech {
            segments.push((start, audio.samples.len()));
        }
        segments
    }

    /// 转录音频文件（读取路径，当前返回音频特征描述）
    pub async fn transcribe_file(&self, audio_path: &std::path::Path) -> anyhow::Result<String> {
        if !audio_path.exists() {
            anyhow::bail!("音频文件不存在: {}", audio_path.display());
        }
        // 未来集成 whisper.cpp 时在此读取文件并转录
        Ok(format!("[STT:file:{}:{}]", audio_path.display(), self.language))
    }

    /// 转录音频数据
    pub async fn transcribe_raw(&self, audio_data: &[f32]) -> anyhow::Result<String> {
        if audio_data.is_empty() {
            anyhow::bail!("音频数据为空");
        }
        let energy = Self::rms_energy(audio_data);
        if energy < self.silence_threshold {
            return Ok(String::new()); // 静音
        }
        // 基于音频特征生成描述（真实 Whisper 集成点）
        Ok(format!(
            "[STT:raw:samples={}:energy={:.4}:lang={}]",
            audio_data.len(),
            energy,
            self.language
        ))
    }

    /// 流式转录
    pub async fn transcribe_stream(&self, audio_stream: &[f32]) -> anyhow::Result<String> {
        self.transcribe_raw(audio_stream).await
    }
}

impl SpeechToTextEngine for SpeechToText {
    fn transcribe(&self, audio: &AudioBuffer) -> anyhow::Result<String> {
        if audio.is_empty() {
            anyhow::bail!("音频数据为空");
        }
        let segments = self.detect_speech_segments(audio);
        if segments.is_empty() {
            return Ok(String::new()); // 全静音
        }
        let total_speech_samples: usize = segments.iter().map(|(s, e)| e - s).sum();
        let speech_ratio = total_speech_samples as f64 / audio.samples.len() as f64;
        Ok(format!(
            "[STT:segments={}:speech_ratio={:.2}:duration={:.2}s:lang={}]",
            segments.len(),
            speech_ratio,
            audio.duration_secs(),
            self.language
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech_audio() -> AudioBuffer {
        // 模拟有语音的音频（高能量）
        let samples: Vec<f32> = (0..16000)
            .map(|i| (i as f32 * 0.1).sin() * 0.5)
            .collect();
        AudioBuffer::new(samples, 16000, 1).unwrap()
    }

    fn silence_audio() -> AudioBuffer {
        AudioBuffer::silence(1.0, 16000, 1)
    }

    #[test]
    fn new_creates_engine() {
        let stt = SpeechToText::new("/tmp/model".into(), "zh");
        assert_eq!(stt.language(), "zh");
        assert!(!stt.is_loaded());
    }

    #[tokio::test]
    async fn load_succeeds() {
        let mut stt = SpeechToText::new("".into(), "zh");
        stt.load().await.unwrap();
        assert!(stt.is_loaded());
    }

    #[test]
    fn transcribe_speech_audio() {
        let stt = SpeechToText::new("".into(), "zh");
        let result = stt.transcribe(&speech_audio()).unwrap();
        assert!(result.contains("STT:segments="));
        assert!(result.contains("lang=zh"));
    }

    #[test]
    fn transcribe_silence_returns_empty() {
        let stt = SpeechToText::new("".into(), "en");
        let result = stt.transcribe(&silence_audio()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn transcribe_empty_audio_fails() {
        let stt = SpeechToText::new("".into(), "zh");
        let empty = AudioBuffer::new(vec![], 16000, 1).unwrap();
        assert!(stt.transcribe(&empty).is_err());
    }

    #[test]
    fn detect_speech_segments_finds_speech() {
        let stt = SpeechToText::new("".into(), "zh");
        let segments = stt.detect_speech_segments(&speech_audio());
        assert!(!segments.is_empty());
    }

    #[test]
    fn detect_speech_segments_silence_empty() {
        let stt = SpeechToText::new("".into(), "zh");
        let segments = stt.detect_speech_segments(&silence_audio());
        assert!(segments.is_empty());
    }

    #[test]
    fn rms_energy_of_silence_is_zero() {
        assert_eq!(SpeechToText::rms_energy(&[0.0; 100]), 0.0);
    }

    #[test]
    fn rms_energy_of_signal_positive() {
        let signal: Vec<f32> = (0..100).map(|i| (i as f32 * 0.1).sin()).collect();
        assert!(SpeechToText::rms_energy(&signal) > 0.0);
    }

    #[tokio::test]
    async fn transcribe_raw_speech() {
        let stt = SpeechToText::new("".into(), "zh");
        let samples: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let result = stt.transcribe_raw(&samples).await.unwrap();
        assert!(result.contains("STT:raw"));
    }

    #[tokio::test]
    async fn transcribe_raw_silence_returns_empty() {
        let stt = SpeechToText::new("".into(), "zh");
        let result = stt.transcribe_raw(&vec![0.0; 1600]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn transcribe_raw_empty_fails() {
        let stt = SpeechToText::new("".into(), "zh");
        assert!(stt.transcribe_raw(&[]).await.is_err());
    }

    #[test]
    fn silence_threshold_configurable() {
        let stt = SpeechToText::new("".into(), "zh").with_silence_threshold(0.5);
        // 低能量信号在高阈值下应被视为静音
        let low_signal: Vec<f32> = (0..16000).map(|i| (i as f32 * 0.1).sin() * 0.1).collect();
        let audio = AudioBuffer::new(low_signal, 16000, 1).unwrap();
        let segments = stt.detect_speech_segments(&audio);
        assert!(segments.is_empty());
    }
}
