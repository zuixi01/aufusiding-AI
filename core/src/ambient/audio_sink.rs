//! 音频输出接收器抽象
//!
//! `AudioSink` trait + `MockAudioSink` 用于测试。

use async_trait::async_trait;

use crate::voice::AudioChunk;

/// 音频输出接收器
#[async_trait]
pub trait AudioSink: Send + Sync {
    /// 播放一个音频块
    async fn play_chunk(&self, chunk: &AudioChunk) -> anyhow::Result<()>;
    /// 立即停止播放（用于 barge-in 打断）
    fn stop_playback(&self);
    /// 是否正在播放
    fn is_playing(&self) -> bool;
}

/// 测试用 Mock 音频接收器
pub struct MockAudioSink {
    played: std::sync::Mutex<Vec<AudioChunk>>,
    playing: std::sync::atomic::AtomicBool,
}

impl MockAudioSink {
    pub fn new() -> Self {
        Self {
            played: std::sync::Mutex::new(Vec::new()),
            playing: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 获取已播放的音频块
    pub fn played_chunks(&self) -> Vec<AudioChunk> {
        self.played
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 已播放块数
    pub fn played_count(&self) -> usize {
        self.played.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

#[async_trait]
impl AudioSink for MockAudioSink {
    async fn play_chunk(&self, chunk: &AudioChunk) -> anyhow::Result<()> {
        self.playing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.played
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(chunk.clone());
        self.playing
            .store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn stop_playback(&self) {
        self.playing
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_playing(&self) -> bool {
        self.playing.load(std::sync::atomic::Ordering::SeqCst)
    }
}
