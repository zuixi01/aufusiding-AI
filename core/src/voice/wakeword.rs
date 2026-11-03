//! 唤醒词检测
//!
//! 基于能量模式匹配的本地唤醒词检测，无需外部依赖。
//! 提供可扩展的 Porcupine 集成点。

use super::traits::WakewordEngine;
use super::types::AudioBuffer;

/// 唤醒词检测器
pub struct WakewordDetector {
    /// 模型路径（未来 Porcupine 集成用）
    model_path: std::path::PathBuf,
    /// 检测阈值（RMS 能量）
    threshold: f32,
    /// 是否已加载
    loaded: bool,
    /// 最小语音时长（秒）— 唤醒词通常 0.3-1.5 秒
    min_speech_duration: f64,
    /// 最大语音时长（秒）
    max_speech_duration: f64,
    /// 最小能量峰值数（唤醒词通常有 2-4 个音节峰值）
    min_peaks: usize,
}

impl WakewordDetector {
    /// 创建唤醒词检测器
    pub fn new(model_path: std::path::PathBuf, threshold: f32) -> Self {
        Self {
            model_path,
            threshold,
            loaded: false,
            min_speech_duration: 0.3,
            max_speech_duration: 2.0,
            min_peaks: 2,
        }
    }

    /// 设置语音时长范围
    pub fn with_duration_range(mut self, min: f64, max: f64) -> Self {
        self.min_speech_duration = min;
        self.max_speech_duration = max;
        self
    }

    /// 设置最小峰值数
    pub fn with_min_peaks(mut self, peaks: usize) -> Self {
        self.min_peaks = peaks;
        self
    }

    /// 是否已加载
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// 加载模型
    pub async fn load(&mut self) -> anyhow::Result<()> {
        if !self.model_path.as_os_str().is_empty() && self.model_path.exists() {
            // 未来集成 Porcupine 时在此加载模型
        }
        self.loaded = true;
        Ok(())
    }

    /// 计算 RMS 能量
    fn rms_energy(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }

    /// 检测能量峰值数（音节检测）
    fn count_energy_peaks(&self, audio: &AudioBuffer) -> usize {
        let frame_size = (audio.sample_rate as usize) / 50; // 20ms 帧
        if frame_size == 0 {
            return 0;
        }
        let energies: Vec<f32> = audio
            .samples
            .chunks(frame_size)
            .map(Self::rms_energy)
            .collect();

        let mut peaks = 0;
        let mut was_above = false;
        for &e in &energies {
            if e > self.threshold {
                if !was_above {
                    peaks += 1;
                    was_above = true;
                }
            } else {
                was_above = false;
            }
        }
        peaks
    }

    /// 计算语音段总时长（秒）
    fn speech_duration(&self, audio: &AudioBuffer) -> f64 {
        let frame_size = (audio.sample_rate as usize) / 50;
        if frame_size == 0 {
            return 0.0;
        }
        let frame_duration = 1.0 / 50.0; // 20ms
        let speech_frames = audio
            .samples
            .chunks(frame_size)
            .filter(|c| Self::rms_energy(c) > self.threshold)
            .count();
        speech_frames as f64 * frame_duration
    }

    /// 处理音频流，检测唤醒词
    pub fn process_audio(&self, audio_data: &[i16]) -> anyhow::Result<bool> {
        if audio_data.is_empty() {
            return Ok(false);
        }
        // i16 → f32 转换
        let samples: Vec<f32> = audio_data.iter().map(|&s| s as f32 / 32768.0).collect();
        let audio = AudioBuffer {
            samples,
            sample_rate: 16000,
            channels: 1,
        };
        self.detect(&audio)
    }

    /// 开始监听（返回就绪状态）
    pub async fn start_listening(&self) -> anyhow::Result<()> {
        if !self.loaded {
            anyhow::bail!("模型未加载，请先调用 load()");
        }
        Ok(())
    }

    /// 停止监听
    pub fn stop_listening(&self) {
        // 清理资源（当前无状态需清理）
    }
}

impl WakewordEngine for WakewordDetector {
    fn detect(&self, audio: &AudioBuffer) -> anyhow::Result<bool> {
        if audio.is_empty() {
            anyhow::bail!("音频数据为空");
        }

        // 1. 检查语音时长是否在唤醒词范围内
        let duration = self.speech_duration(audio);
        if duration < self.min_speech_duration || duration > self.max_speech_duration {
            return Ok(false);
        }

        // 2. 检查能量峰值数（音节数）
        let peaks = self.count_energy_peaks(audio);
        if peaks < self.min_peaks {
            return Ok(false);
        }

        // 3. 整体能量检查
        let overall_energy = Self::rms_energy(&audio.samples);
        if overall_energy < self.threshold * 0.5 {
            return Ok(false);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wakeword_audio(sample_rate: u32) -> AudioBuffer {
        // 模拟 "嘿灵犀" — 3 个音节，约 0.6 秒
        let duration = 0.6;
        let num_samples = (duration * sample_rate as f64) as usize;
        let mut samples = vec![0.0f32; num_samples];

        // 3 个音节峰值，每个约 0.15 秒，间隔 0.05 秒静音
        let syllable_len = (0.15 * sample_rate as f64) as usize;
        let gap_len = (0.05 * sample_rate as f64) as usize;
        for syllable in 0..3 {
            let start = syllable * (syllable_len + gap_len);
            for i in 0..syllable_len.min(num_samples - start) {
                let t = i as f32 / sample_rate as f32;
                samples[start + i] = (2.0 * std::f32::consts::PI * 200.0 * t).sin() * 0.5;
            }
        }
        AudioBuffer::new(samples, sample_rate, 1).unwrap()
    }

    fn make_silence() -> AudioBuffer {
        AudioBuffer::silence(1.0, 16000, 1)
    }

    fn make_noise(duration: f64) -> AudioBuffer {
        // 持续噪声（无音节结构）
        let num = (duration * 16000.0) as usize;
        let samples: Vec<f32> = (0..num).map(|i| (i as f32 * 0.3).sin() * 0.4).collect();
        AudioBuffer::new(samples, 16000, 1).unwrap()
    }

    #[test]
    fn new_creates_detector() {
        let det = WakewordDetector::new("/tmp/model".into(), 0.1);
        assert!(!det.is_loaded());
    }

    #[tokio::test]
    async fn load_succeeds() {
        let mut det = WakewordDetector::new("".into(), 0.1);
        det.load().await.unwrap();
        assert!(det.is_loaded());
    }

    #[test]
    fn detect_wakeword_audio() {
        let det = WakewordDetector::new("".into(), 0.05);
        let audio = make_wakeword_audio(16000);
        assert!(det.detect(&audio).unwrap());
    }

    #[test]
    fn reject_silence() {
        let det = WakewordDetector::new("".into(), 0.05);
        assert!(!det.detect(&make_silence()).unwrap());
    }

    #[test]
    fn reject_empty_audio() {
        let det = WakewordDetector::new("".into(), 0.05);
        let empty = AudioBuffer::new(vec![], 16000, 1).unwrap();
        assert!(det.detect(&empty).is_err());
    }

    #[test]
    fn reject_too_long_speech() {
        let det = WakewordDetector::new("".into(), 0.05);
        // 5 秒持续语音 — 太长不是唤醒词
        let audio = make_noise(5.0);
        assert!(!det.detect(&audio).unwrap());
    }

    #[test]
    fn reject_single_peak() {
        let det = WakewordDetector::new("".into(), 0.05).with_min_peaks(2);
        // 单个短脉冲
        let mut samples = vec![0.0f32; 16000];
        for i in 0..800 {
            samples[i] = (i as f32 * 0.2).sin() * 0.5;
        }
        let audio = AudioBuffer::new(samples, 16000, 1).unwrap();
        assert!(!det.detect(&audio).unwrap());
    }

    #[test]
    fn process_audio_i16() {
        let det = WakewordDetector::new("".into(), 0.05);
        // 模拟 i16 音频数据（3 个音节）
        let mut data = vec![0i16; 16000];
        let syllable_len = 2400;
        let gap = 800;
        for s in 0..3 {
            let start = s * (syllable_len + gap);
            for i in 0..syllable_len {
                data[start + i] = ((i as f32 * 0.2).sin() * 16000.0) as i16;
            }
        }
        assert!(det.process_audio(&data).unwrap());
    }

    #[test]
    fn process_audio_empty() {
        let det = WakewordDetector::new("".into(), 0.05);
        assert!(!det.process_audio(&[]).unwrap());
    }

    #[tokio::test]
    async fn start_listening_requires_load() {
        let det = WakewordDetector::new("".into(), 0.1);
        assert!(det.start_listening().await.is_err());
    }

    #[tokio::test]
    async fn start_listening_after_load() {
        let mut det = WakewordDetector::new("".into(), 0.1);
        det.load().await.unwrap();
        assert!(det.start_listening().await.is_ok());
    }

    #[test]
    fn duration_range_configurable() {
        let det = WakewordDetector::new("".into(), 0.05).with_duration_range(0.5, 1.0);
        // 0.6 秒唤醒词应通过
        let audio = make_wakeword_audio(16000);
        let dur = det.speech_duration(&audio);
        assert!(dur >= 0.3); // 实际语音段时长
    }

    #[test]
    fn count_peaks_correct() {
        let det = WakewordDetector::new("".into(), 0.05);
        let audio = make_wakeword_audio(16000);
        let peaks = det.count_energy_peaks(&audio);
        assert!(peaks >= 2, "expected >= 2 peaks, got {}", peaks);
    }

    #[test]
    fn rms_energy_silence_zero() {
        assert_eq!(WakewordDetector::rms_energy(&[0.0; 100]), 0.0);
    }

    #[test]
    fn rms_energy_signal_positive() {
        let signal: Vec<f32> = (0..100).map(|i| (i as f32 * 0.1).sin()).collect();
        assert!(WakewordDetector::rms_energy(&signal) > 0.0);
    }
}
