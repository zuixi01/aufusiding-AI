//! 系统音频输入源适配器
//!
//! `SystemAudioSource` 通过系统音频 API (ALSA/PulseAudio/PipeWire)
//! 实现 `AudioSource` trait，提供麦克风音频采集能力。

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tokio::process::Command;

use crate::voice::AudioChunk;

use super::audio_source::AudioSource;

/// 音频输入后端
#[derive(Debug, Clone, PartialEq)]
enum AudioInputBackend {
    /// PipeWire (pw-record)
    PipeWire,
    /// PulseAudio (parecord)
    PulseAudio,
    /// ALSA (arecord)
    Alsa,
}

/// 系统音频输入源配置
#[derive(Debug, Clone)]
pub struct AudioSourceConfig {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for AudioSourceConfig {
    fn default() -> Self {
        Self {
            device_name: "default".to_string(),
            sample_rate: 16000,
            channels: 1,
        }
    }
}

/// 系统音频输入源
///
/// 通过调用系统录音工具采集音频数据，自动检测可用后端。
pub struct SystemAudioSource {
    config: AudioSourceConfig,
    backend: AudioInputBackend,
    active: AtomicBool,
    sequence: Mutex<u64>,
}

impl SystemAudioSource {
    pub fn new(config: AudioSourceConfig) -> Self {
        let backend = Self::detect_backend();
        Self {
            config,
            backend,
            active: AtomicBool::new(false),
            sequence: Mutex::new(0),
        }
    }

    /// 检测可用的音频后端
    fn detect_backend() -> AudioInputBackend {
        // 优先级: PipeWire > PulseAudio > ALSA
        if std::process::Command::new("pw-record")
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            AudioInputBackend::PipeWire
        } else if std::process::Command::new("parecord")
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            AudioInputBackend::PulseAudio
        } else {
            AudioInputBackend::Alsa
        }
    }

    /// 构建录音命令（录制一个短块）
    fn build_record_command(&self) -> Command {
        let duration_ms = 30; // 30ms 块
        let duration_sec = duration_ms as f64 / 1000.0;

        match &self.backend {
            AudioInputBackend::PipeWire => {
                let mut cmd = Command::new("pw-record");
                cmd.args([
                    "--format",
                    "f32",
                    "--rate",
                    &self.config.sample_rate.to_string(),
                    "--channels",
                    &self.config.channels.to_string(),
                    "--target",
                    &self.config.device_name,
                    "-",
                ]);
                cmd
            }
            AudioInputBackend::PulseAudio => {
                let mut cmd = Command::new("parecord");
                cmd.args([
                    "--format=float32le",
                    &format!("--rate={}", self.config.sample_rate),
                    &format!("--channels={}", self.config.channels),
                    &format!("--device={}", self.config.device_name),
                    &format!("--process-time-msec={}", duration_ms),
                    "-",
                ]);
                cmd
            }
            AudioInputBackend::Alsa => {
                let mut cmd = Command::new("arecord");
                cmd.args([
                    "-f",
                    "FLOAT_LE",
                    "-r",
                    &self.config.sample_rate.to_string(),
                    "-c",
                    &self.config.channels.to_string(),
                    "-D",
                    &self.config.device_name,
                    "-d",
                    &format!("{:.3}", duration_sec),
                    "-t",
                    "raw",
                    "-q",
                    "-",
                ]);
                cmd
            }
        }
    }

    /// 将原始字节转换为 f32 样本
    fn bytes_to_samples(data: &[u8]) -> Vec<f32> {
        data.as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(f32::from_le_bytes)
            .collect()
    }
}

#[async_trait]
impl AudioSource for SystemAudioSource {
    async fn read_chunk(&self) -> anyhow::Result<AudioChunk> {
        self.active.store(true, Ordering::SeqCst);

        let output = self
            .build_record_command()
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .await?;

        if !output.status.success() {
            self.active.store(false, Ordering::SeqCst);
            anyhow::bail!("音频录制命令失败 (backend={:?})", self.backend);
        }

        let samples = Self::bytes_to_samples(&output.stdout);
        let mut seq = self.sequence.lock().unwrap_or_else(|e| e.into_inner());
        let current_seq = *seq;
        *seq += 1;

        let timestamp_ms = current_seq * 30; // 每块 30ms

        Ok(AudioChunk::new(
            samples,
            self.config.sample_rate,
            self.config.channels,
            current_seq,
            timestamp_ms,
        ))
    }

    fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = AudioSourceConfig::default();
        assert_eq!(config.device_name, "default");
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
    }

    #[test]
    fn bytes_to_samples_converts_correctly() {
        let val: f32 = 0.5;
        let bytes = val.to_le_bytes();
        let mut data = Vec::new();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(&bytes);

        let samples = SystemAudioSource::bytes_to_samples(&data);
        assert_eq!(samples.len(), 2);
        assert!((samples[0] - 0.5).abs() < f32::EPSILON);
        assert!((samples[1] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn bytes_to_samples_ignores_incomplete_trailing_sample() {
        for trailing_bytes in 1..4 {
            let mut data = 0.5_f32.to_le_bytes().to_vec();
            data.extend_from_slice(&[0xff; 3][..trailing_bytes]);
            assert_eq!(SystemAudioSource::bytes_to_samples(&data), vec![0.5]);
            assert!(SystemAudioSource::bytes_to_samples(&data[..trailing_bytes]).is_empty());
        }
    }

    #[test]
    fn source_initial_state() {
        let source = SystemAudioSource::new(AudioSourceConfig::default());
        assert!(!source.is_active());

        source.active.store(true, Ordering::SeqCst);
        assert!(source.is_active());

        source.stop();
        assert!(!source.is_active());
    }
}
