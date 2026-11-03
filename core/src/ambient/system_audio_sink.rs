//! 系统音频输出接收器适配器
//!
//! `SystemAudioSink` 通过系统音频 API (ALSA/PulseAudio/PipeWire)
//! 实现 `AudioSink` trait，提供音频播放能力。

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::process::Command;

use crate::voice::AudioChunk;

use super::audio_sink::AudioSink;

/// 音频输出后端
#[derive(Debug, Clone, PartialEq)]
enum AudioOutputBackend {
    /// PipeWire (pw-play)
    PipeWire,
    /// PulseAudio (paplay)
    PulseAudio,
    /// ALSA (aplay)
    Alsa,
}

/// 系统音频输出配置
#[derive(Debug, Clone)]
pub struct AudioSinkConfig {
    pub device_name: String,
    pub sample_rate: u32,
}

impl Default for AudioSinkConfig {
    fn default() -> Self {
        Self {
            device_name: "default".to_string(),
            sample_rate: 16000,
        }
    }
}

/// 系统音频输出接收器
///
/// 通过调用系统播放工具输出音频数据，自动检测可用后端。
pub struct SystemAudioSink {
    config: AudioSinkConfig,
    backend: AudioOutputBackend,
    playing: AtomicBool,
}

impl SystemAudioSink {
    pub fn new(config: AudioSinkConfig) -> Self {
        let backend = Self::detect_backend();
        Self {
            config,
            backend,
            playing: AtomicBool::new(false),
        }
    }

    /// 检测可用的音频后端
    fn detect_backend() -> AudioOutputBackend {
        if std::process::Command::new("pw-play")
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            AudioOutputBackend::PipeWire
        } else if std::process::Command::new("paplay")
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            AudioOutputBackend::PulseAudio
        } else {
            AudioOutputBackend::Alsa
        }
    }

    /// 将 f32 样本转换为原始字节 (little-endian)
    fn samples_to_bytes(samples: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for &s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        bytes
    }

    /// 构建播放命令
    fn build_play_command(&self, chunk: &AudioChunk) -> (Command, Vec<u8>) {
        let raw_data = Self::samples_to_bytes(&chunk.samples);

        let cmd = match &self.backend {
            AudioOutputBackend::PipeWire => {
                let mut c = Command::new("pw-play");
                c.args([
                    "--format",
                    "f32",
                    "--rate",
                    &chunk.sample_rate.to_string(),
                    "--channels",
                    &chunk.channels.to_string(),
                    "--target",
                    &self.config.device_name,
                    "-",
                ]);
                c
            }
            AudioOutputBackend::PulseAudio => {
                let mut c = Command::new("paplay");
                c.args([
                    "--format=float32le",
                    &format!("--rate={}", chunk.sample_rate),
                    &format!("--channels={}", chunk.channels),
                    &format!("--device={}", self.config.device_name),
                    "--raw",
                ]);
                c
            }
            AudioOutputBackend::Alsa => {
                let mut c = Command::new("aplay");
                c.args([
                    "-f",
                    "FLOAT_LE",
                    "-r",
                    &chunk.sample_rate.to_string(),
                    "-c",
                    &chunk.channels.to_string(),
                    "-D",
                    &self.config.device_name,
                    "-t",
                    "raw",
                    "-q",
                ]);
                c
            }
        };

        (cmd, raw_data)
    }
}

#[async_trait]
impl AudioSink for SystemAudioSink {
    async fn play_chunk(&self, chunk: &AudioChunk) -> anyhow::Result<()> {
        if chunk.is_empty() {
            return Ok(());
        }

        self.playing.store(true, Ordering::SeqCst);

        let (mut cmd, raw_data) = self.build_play_command(chunk);

        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(&raw_data).await?;
            drop(stdin);
        }

        let status = child.wait().await?;
        self.playing.store(false, Ordering::SeqCst);

        if !status.success() {
            anyhow::bail!("音频播放命令失败 (backend={:?})", self.backend);
        }

        Ok(())
    }

    fn stop_playback(&self) {
        self.playing.store(false, Ordering::SeqCst);
        // 实际实现中需要终止子进程
    }

    fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = AudioSinkConfig::default();
        assert_eq!(config.device_name, "default");
        assert_eq!(config.sample_rate, 16000);
    }

    #[test]
    fn samples_to_bytes_roundtrip() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0];
        let bytes = SystemAudioSink::samples_to_bytes(&samples);
        assert_eq!(bytes.len(), 16); // 4 samples * 4 bytes

        // 验证往返转换
        let restored: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(restored, samples);
    }

    #[test]
    fn sink_initial_state() {
        let sink = SystemAudioSink::new(AudioSinkConfig::default());
        assert!(!sink.is_playing());

        sink.playing.store(true, Ordering::SeqCst);
        assert!(sink.is_playing());

        sink.stop_playback();
        assert!(!sink.is_playing());
    }
}
