//! cpal 低延迟音频输入源
//!
//! 使用 cpal 库直接访问音频设备，提供低延迟跨平台音频采集。
//! Stream 在独立线程中管理，结构体本身满足 Send + Sync。

#![cfg(feature = "audio-capture")]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::audio_source::AudioSource;
use crate::voice::AudioChunk;

/// cpal 音频配置
#[derive(Debug, Clone)]
pub struct CpalAudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub chunk_samples: usize,
}

impl Default for CpalAudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            chunk_samples: 480, // 30ms @ 16kHz
        }
    }
}

/// 共享状态（Send + Sync 安全）
struct SharedInputState {
    buffer: Mutex<VecDeque<f32>>,
    active: AtomicBool,
    sequence: Mutex<u64>,
}

/// cpal 低延迟音频输入源
pub struct CpalAudioSource {
    config: CpalAudioConfig,
    state: Arc<SharedInputState>,
    /// 停止信号，drop 时通知流线程退出
    _stop_tx: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl CpalAudioSource {
    /// 创建但不启动流
    pub fn new(config: CpalAudioConfig) -> Self {
        Self {
            config,
            state: Arc::new(SharedInputState {
                buffer: Mutex::new(VecDeque::with_capacity(4800)),
                active: AtomicBool::new(false),
                sequence: Mutex::new(0),
            }),
            _stop_tx: Mutex::new(None),
        }
    }

    /// 启动音频采集流（在独立线程中运行 cpal stream）
    pub fn start(&self) -> anyhow::Result<()> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("无可用输入设备"))?;

        let supported = device.supported_input_configs()?;
        let stream_config =
            find_matching_config(supported, self.config.sample_rate, self.config.channels)?;

        let state = Arc::clone(&self.state);
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

        std::thread::spawn(move || {
            let buffer = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(4800)));
            let buf_clone = Arc::clone(&buffer);

            let stream = device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut buf) = buf_clone.lock() {
                        buf.extend(data.iter());
                        while buf.len() > 16000 {
                            buf.pop_front();
                        }
                    }
                },
                |err| {
                    tracing::error!(error = %err, "cpal 输入流错误");
                },
                None,
            );

            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "无法创建输入流");
                    return;
                }
            };

            if stream.play().is_err() {
                return;
            }

            // 持续将 cpal buffer 转移到共享 state buffer
            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                if let Ok(mut src) = buffer.lock() {
                    if !src.is_empty() {
                        if let Ok(mut dst) = state.buffer.lock() {
                            dst.extend(src.drain(..));
                            while dst.len() > 16000 {
                                dst.pop_front();
                            }
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            drop(stream);
        });

        self.state.active.store(true, Ordering::SeqCst);
        *self._stop_tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(stop_tx);
        Ok(())
    }

    /// 列出可用输入设备
    pub fn list_input_devices() -> Vec<String> {
        use cpal::traits::HostTrait;
        let host = cpal::default_host();
        host.input_devices()
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
impl AudioSource for CpalAudioSource {
    async fn read_chunk(&self) -> anyhow::Result<AudioChunk> {
        let needed = self.config.chunk_samples;
        loop {
            {
                let mut buf = self.state.buffer.lock().unwrap_or_else(|e| e.into_inner());
                if buf.len() >= needed {
                    let samples: Vec<f32> = buf.drain(..needed).collect();
                    let mut seq = self
                        .state
                        .sequence
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let seq_id = *seq;
                    *seq += 1;
                    return Ok(AudioChunk::new(
                        samples,
                        self.config.sample_rate,
                        self.config.channels,
                        seq_id,
                        seq_id * 30,
                    ));
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
    }

    fn stop(&self) {
        self.state.active.store(false, Ordering::SeqCst);
        // 发送停止信号
        if let Some(tx) = self
            ._stop_tx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = tx.send(());
        }
        self.state
            .buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    fn is_active(&self) -> bool {
        self.state.active.load(Ordering::SeqCst)
    }
}

/// 从支持的配置中找到匹配的
fn find_matching_config(
    supported: cpal::SupportedInputConfigs,
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
        "未找到匹配的输入配置 ({}Hz, {}ch, f32)",
        sample_rate,
        channels
    ))
}

/// f32 样本与字节的转换工具
pub fn samples_to_bytes(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

pub fn bytes_to_samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let cfg = CpalAudioConfig::default();
        assert_eq!(cfg.sample_rate, 16000);
        assert_eq!(cfg.channels, 1);
        assert_eq!(cfg.chunk_samples, 480);
    }

    #[test]
    fn initial_state_inactive() {
        let source = CpalAudioSource::new(CpalAudioConfig::default());
        assert!(!source.is_active());
    }

    #[test]
    fn stop_sets_inactive() {
        let source = CpalAudioSource::new(CpalAudioConfig::default());
        source.state.active.store(true, Ordering::SeqCst);
        assert!(source.is_active());
        source.stop();
        assert!(!source.is_active());
    }

    #[test]
    fn f32_bytes_roundtrip() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let bytes = samples_to_bytes(&samples);
        let recovered = bytes_to_samples(&bytes);
        assert_eq!(samples.len(), recovered.len());
        for (a, b) in samples.iter().zip(recovered.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn list_input_devices_no_panic() {
        let devices = CpalAudioSource::list_input_devices();
        let _ = devices;
    }

    #[test]
    fn custom_sample_rate() {
        let cfg = CpalAudioConfig {
            sample_rate: 44100,
            ..Default::default()
        };
        let source = CpalAudioSource::new(cfg);
        assert_eq!(source.config().sample_rate, 44100);
    }

    #[test]
    fn custom_channels() {
        let cfg = CpalAudioConfig {
            channels: 2,
            ..Default::default()
        };
        let source = CpalAudioSource::new(cfg);
        assert_eq!(source.config().channels, 2);
    }

    #[test]
    fn implements_audio_source_trait() {
        let source = CpalAudioSource::new(CpalAudioConfig::default());
        let _: Box<dyn AudioSource> = Box::new(source);
    }
}
