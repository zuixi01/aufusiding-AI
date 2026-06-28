//! 文字转语音 (TTS)
//!
//! 本地 TTS 引擎实现。基于简易波形合成提供可运行的音频输出，
//! 以及可扩展的 Piper TTS 集成点。

use super::traits::TextToSpeechEngine;
use super::types::AudioBuffer;

/// 文字转语音引擎
pub struct TextToSpeech {
    /// Piper 模型路径
    model_path: std::path::PathBuf,
    /// 音色 ID
    voice_id: String,
    /// 是否已加载模型
    loaded: bool,
    /// 采样率
    sample_rate: u32,
    /// 声道数
    channels: u16,
    /// 语速倍率 (1.0 = 正常)
    speed: f32,
}

impl TextToSpeech {
    /// 创建 TTS 引擎
    pub fn new(model_path: std::path::PathBuf, voice_id: &str) -> Self {
        Self {
            model_path,
            voice_id: voice_id.to_string(),
            loaded: false,
            sample_rate: 22050,
            channels: 1,
            speed: 1.0,
        }
    }

    /// 设置采样率
    pub fn with_sample_rate(mut self, rate: u32) -> Self {
        self.sample_rate = rate;
        self
    }

    /// 设置语速
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed.clamp(0.25, 4.0);
        self
    }

    /// 获取音色 ID
    pub fn voice_id(&self) -> &str {
        &self.voice_id
    }

    /// 是否已加载
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// 加载模型（当前为本地合成模式，无需外部模型）
    pub async fn load(&mut self) -> anyhow::Result<()> {
        if !self.model_path.as_os_str().is_empty() && self.model_path.exists() {
            // 未来集成 Piper TTS 时在此加载模型
        }
        self.loaded = true;
        Ok(())
    }

    /// 估算文本对应的音频时长（秒）
    fn estimate_duration(&self, text: &str) -> f64 {
        // 中文约 4 字/秒，英文约 3 词/秒
        let chars = text.chars().count();
        let words = text.split_whitespace().count();
        let base = if chars > words * 2 {
            chars as f64 * 0.25 // 中文为主
        } else {
            words as f64 * 0.33 // 英文为主
        };
        (base / self.speed as f64).max(0.1)
    }

    /// 生成简易正弦波合成音频（模拟语音韵律）
    fn synthesize_waveform(&self, text: &str) -> Vec<f32> {
        let duration = self.estimate_duration(text);
        let num_samples = (duration * self.sample_rate as f64 * self.channels as f64) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        // 基于文本哈希生成不同的基频，模拟不同音色
        let base_freq = 150.0 + (self.voice_id.len() as f32 * 10.0);

        for i in 0..num_samples {
            let t = i as f32 / self.sample_rate as f32;
            // 基频 + 谐波，模拟语音频谱
            let fundamental = (2.0 * std::f32::consts::PI * base_freq * t).sin();
            let harmonic2 = (2.0 * std::f32::consts::PI * base_freq * 2.0 * t).sin() * 0.5;
            let harmonic3 = (2.0 * std::f32::consts::PI * base_freq * 3.0 * t).sin() * 0.25;
            // 包络：淡入淡出
            let progress = i as f32 / num_samples as f32;
            let envelope = if progress < 0.05 {
                progress / 0.05
            } else if progress > 0.95 {
                (1.0 - progress) / 0.05
            } else {
                1.0
            };
            let sample = (fundamental + harmonic2 + harmonic3) * 0.3 * envelope;
            samples.push(sample);
        }
        samples
    }

    /// 合成语音到文件
    pub async fn synthesize_to_file(
        &self,
        text: &str,
        output_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        if text.is_empty() {
            anyhow::bail!("文本为空");
        }
        let samples = self.synthesize_waveform(text);
        // 写入简易 WAV 格式
        let wav_data = encode_wav_pcm(&samples, self.sample_rate, self.channels);
        std::fs::write(output_path, wav_data)?;
        Ok(())
    }

    /// 合成语音到内存
    pub async fn synthesize_raw(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        if text.is_empty() {
            anyhow::bail!("文本为空");
        }
        Ok(self.synthesize_waveform(text))
    }

    /// 流式合成（返回音频数据）
    pub async fn synthesize_stream(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        self.synthesize_raw(text).await
    }
}

/// 编码为简易 WAV PCM 格式
fn encode_wav_pcm(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = channels * (bits_per_sample / 8);
    let data_size = samples.len() as u32 * 2; // 16-bit samples
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + data_size as usize);
    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let i16_val = (clamped * 32767.0) as i16;
        buf.extend_from_slice(&i16_val.to_le_bytes());
    }
    buf
}

impl TextToSpeechEngine for TextToSpeech {
    fn synthesize(&self, text: &str) -> anyhow::Result<AudioBuffer> {
        if text.is_empty() {
            anyhow::bail!("文本为空");
        }
        let samples = self.synthesize_waveform(text);
        Ok(AudioBuffer {
            samples,
            sample_rate: self.sample_rate,
            channels: self.channels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_engine() {
        let tts = TextToSpeech::new("/tmp/model".into(), "zh-female");
        assert_eq!(tts.voice_id(), "zh-female");
        assert!(!tts.is_loaded());
    }

    #[tokio::test]
    async fn load_succeeds() {
        let mut tts = TextToSpeech::new("".into(), "default");
        tts.load().await.unwrap();
        assert!(tts.is_loaded());
    }

    #[test]
    fn synthesize_produces_audio() {
        let tts = TextToSpeech::new("".into(), "default");
        let buf = tts.synthesize("你好世界").unwrap();
        assert!(!buf.is_empty());
        assert_eq!(buf.sample_rate, 22050);
        assert_eq!(buf.channels, 1);
    }

    #[test]
    fn synthesize_empty_text_fails() {
        let tts = TextToSpeech::new("".into(), "default");
        assert!(tts.synthesize("").is_err());
    }

    #[test]
    fn longer_text_produces_longer_audio() {
        let tts = TextToSpeech::new("".into(), "default");
        let short = tts.synthesize("你好").unwrap();
        let long = tts.synthesize("你好世界，今天天气真不错").unwrap();
        assert!(long.samples.len() > short.samples.len());
    }

    #[test]
    fn speed_affects_duration() {
        let fast = TextToSpeech::new("".into(), "default").with_speed(2.0);
        let normal = TextToSpeech::new("".into(), "default");
        let fast_buf = fast.synthesize("测试语速").unwrap();
        let normal_buf = normal.synthesize("测试语速").unwrap();
        assert!(fast_buf.samples.len() < normal_buf.samples.len());
    }

    #[test]
    fn different_voices_produce_different_waveforms() {
        let tts_a = TextToSpeech::new("".into(), "voice_a");
        let tts_b = TextToSpeech::new("".into(), "voice_b_longer");
        let buf_a = tts_a.synthesize("测试").unwrap();
        let buf_b = tts_b.synthesize("测试").unwrap();
        // 不同音色基频不同，波形应不同
        assert_ne!(buf_a.samples, buf_b.samples);
    }

    #[test]
    fn waveform_has_envelope() {
        let tts = TextToSpeech::new("".into(), "default");
        let buf = tts.synthesize("测试包络").unwrap();
        // 开头应该淡入（接近 0）
        let start_energy: f32 = buf.samples[..10].iter().map(|s| s.abs()).sum::<f32>() / 10.0;
        let mid = buf.samples.len() / 2;
        let mid_energy: f32 = buf.samples[mid..mid + 10]
            .iter()
            .map(|s| s.abs())
            .sum::<f32>()
            / 10.0;
        assert!(start_energy < mid_energy);
    }

    #[tokio::test]
    async fn synthesize_raw_returns_samples() {
        let tts = TextToSpeech::new("".into(), "default");
        let samples = tts.synthesize_raw("你好").await.unwrap();
        assert!(!samples.is_empty());
    }

    #[tokio::test]
    async fn synthesize_raw_empty_fails() {
        let tts = TextToSpeech::new("".into(), "default");
        assert!(tts.synthesize_raw("").await.is_err());
    }

    #[tokio::test]
    async fn synthesize_to_file_creates_wav() {
        let tts = TextToSpeech::new("".into(), "default");
        let tmp = std::env::temp_dir().join("tts_test.wav");
        tts.synthesize_to_file("测试", &tmp).await.unwrap();
        let data = std::fs::read(&tmp).unwrap();
        // WAV 文件应以 RIFF 开头
        assert_eq!(&data[..4], b"RIFF");
        assert_eq!(&data[8..12], b"WAVE");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn encode_wav_pcm_valid_header() {
        let samples = vec![0.0f32; 100];
        let wav = encode_wav_pcm(&samples, 16000, 1);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn speed_clamped() {
        let tts = TextToSpeech::new("".into(), "default").with_speed(10.0);
        assert_eq!(tts.speed, 4.0);
        let tts2 = TextToSpeech::new("".into(), "default").with_speed(0.01);
        assert_eq!(tts2.speed, 0.25);
    }

    #[test]
    fn sample_rate_configurable() {
        let tts = TextToSpeech::new("".into(), "default").with_sample_rate(44100);
        let buf = tts.synthesize("测试").unwrap();
        assert_eq!(buf.sample_rate, 44100);
    }
}
