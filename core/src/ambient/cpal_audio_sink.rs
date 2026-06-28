//! cpal 低延迟音频输出
//!
//! 使用 cpal 库直接访问音频设备，提供低延迟跨平台音频播放。
//! Stream 在独立线程中管理，结构体本身满足 Send + Sync。

#![cfg(feature = "audio-capture")]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::voice::AudioChunk;
use super::audio_sink::AudioSink;
use super::cpal_audio_source::CpalAudioConfig;

/// 共享输出状态（Send + Sync 安全）
struct SharedOutputState {
    buffer: Mutex<VecDeque<f32>>,
    playing: AtomicBool,
}

/// cpal 低延迟音频输出
pub struct CpalAudioSink {
    config: CpalAudioConfig,
    state: Arc<SharedOutputState>,
    _stop_tx: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl CpalAudioSink {
    /// 创建但不启动流
    pub fn new(config: CpalAudioConfig) -> Self {
        Self {
            config,
            state: Arc::new(SharedOutputState {
                buffer: Mutex::new(VecDeque::with_capacity(4800)),
                playing: AtomicBool::new(false),
            }),
            _stop_tx: Mutex::new(None),
        }
    }

    /// 启动音频输出流（在独立线程中运行 cpal stream）
    pub fn start(&self) -> anyhow::Result<()> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("无可用输出设备"))?;

        let supported = device.supported_output_configs()?;
        let stream_config = find_matching_output_config(
            supported,
            self.config.sample_rate,
            self.config.channels,
        )?;

        let state = Arc::clone(&self.state);
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

        std::thread::spawn(move || {
            let state_clone = Arc::clone(&state);

            let stream = device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = state_clone.buffer.lock().unwrap_or_else(|e| e.into_inner());
                    for sample in data.iter_mut() {
                        *sample = buf.pop_front().unwrap_or(0.0);
                    }
                    if buf.is_empty() {
                        state_clone.playing.store(false, Ordering::SeqCst);
                    }
                },
                |err| {
                    tracing::error!(error = %err, "cpal 输出流错误");
                },
                None,
            );

            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "无法创建输出流");
                    return;
                }
            };

            if stream.play().is_err() {
                return;
            }

            // 保持线程存活直到收到停止信号
            let _ = stop_rx.recv();
            drop(stream);
        });

        *self._stop_tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(stop_tx);
        Ok(())
    }

    /// 列出可用输出设备
    pub fn list_output_devices() -> Vec<String> {
        use cpal::traits::HostTrait;
        let host = cpal::default_host();
        host.output_devices()
            .map(|devices| {
                use cpal::traits::DeviceTrait;
                devices.filter_map(|d| d.name().ok()).collect()
            })
            .unwrap_or_default()
    }

    /// 获取配置
    pub fn config(&self) -> &CpalAudioConfig {
        &self.config
    }
}

#[async_trait]
impl AudioSink for CpalAudioSink {
    async fn play_chunk(&self, chunk: &AudioChunk) -> anyhow::Result<()> {
        if !chunk.samples.is_empty() {
            self.state.playing.store(true, Ordering::SeqCst);
            let mut buf = self.state.buffer.lock().unwrap_or_else(|e| e.into_inner());
            buf.extend(chunk.samples.iter());
        }
        Ok(())
    }

    fn stop_playback(&self) {
        self.state.playing.store(false, Ordering::SeqCst);
        self.state.buffer.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    fn is_playing(&self) -> bool {
        self.state.playing.load(Ordering::SeqCst)
    }
}

/// 从支持的配置中找到匹配的输出配置
fn find_matching_output_config(
    supported: cpal::SupportedOutputConfigs,
    sample_rate: u32,
    channels: u16,
) -> anyhow::Result<cpal::StreamConfig> {
    use cpal::SampleFormat;
    let rate = cpal::SampleRate(sample_rate);
    for cfg in supported {
        if cfg.channels() == channels
            && cfg.sample_format() == SampleFormat::F32
            && cfg.min_sample_rate() <= rate
            && cfg.max_sample_rate() >= rate
        {
            return Ok(cfg.with_sample_rate(rate).into());
        }
    }
    Err(anyhow::anyhow!(
        "未找到匹配的输出配置 ({}Hz, {}ch, f32)",
        sample_rate,
        channels
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let cfg = CpalAudioConfig::default();
        assert_eq!(cfg.sample_rate, 16000);
        assert_eq!(cfg.channels, 1);
    }

    #[test]
    fn initial_state_not_playing() {
        let sink = CpalAudioSink::new(CpalAudioConfig::default());
        assert!(!sink.is_playing());
    }

    #[test]
    fn stop_playback_clears() {
        let sink = CpalAudioSink::new(CpalAudioConfig::default());
        sink.state.playing.store(true, Ordering::SeqCst);
        sink.state.buffer.lock().unwrap().extend(&[0.1, 0.2, 0.3]);
        sink.stop_playback();
        assert!(!sink.is_playing());
        assert!(sink.state.buffer.lock().unwrap().is_empty());
    }

    #[test]
    fn samples_to_bytes_roundtrip() {
        use super::super::cpal_audio_source::{bytes_to_samples, samples_to_bytes};
        let samples = vec![0.1f32, -0.2, 0.3, -0.4];
        let bytes = samples_to_bytes(&samples);
        let recovered = bytes_to_samples(&bytes);
        for (a, b) in samples.iter().zip(recovered.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[tokio::test]
    async fn play_empty_chunk_noop() {
        let sink = CpalAudioSink::new(CpalAudioConfig::default());
        let chunk = AudioChunk::new(vec![], 16000, 1, 0, 0);
        let result = sink.play_chunk(&chunk).await;
        assert!(result.is_ok());
        assert!(!sink.is_playing());
    }

    #[test]
    fn list_output_devices_no_panic() {
        let devices = CpalAudioSink::list_output_devices();
        let _ = devices;
    }

    #[test]
    fn implements_audio_sink_trait() {
        let sink = CpalAudioSink::new(CpalAudioConfig::default());
        let _: Box<dyn AudioSink> = Box::new(sink);
    }
}
