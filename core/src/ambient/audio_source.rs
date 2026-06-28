//! 音频输入源抽象
//!
//! `AudioSource` trait + `MockAudioSource` 用于测试。

use async_trait::async_trait;

use crate::voice::AudioChunk;

/// 音频输入源
#[async_trait]
pub trait AudioSource: Send + Sync {
    /// 读取下一个音频块，阻塞直到有数据
    async fn read_chunk(&self) -> anyhow::Result<AudioChunk>;
    /// 停止采集
    fn stop(&self);
    /// 是否正在采集
    fn is_active(&self) -> bool;
}

/// 测试用 Mock 音频源
pub struct MockAudioSource {
    chunks: std::sync::Mutex<Vec<AudioChunk>>,
    index: std::sync::atomic::AtomicUsize,
}

impl MockAudioSource {
    pub fn new(chunks: Vec<AudioChunk>) -> Self {
        Self {
            chunks: std::sync::Mutex::new(chunks),
            index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 追加更多音频块
    pub fn push_chunk(&self, chunk: AudioChunk) {
        self.chunks.lock().unwrap_or_else(|e| e.into_inner()).push(chunk);
    }
}

#[async_trait]
impl AudioSource for MockAudioSource {
    async fn read_chunk(&self) -> anyhow::Result<AudioChunk> {
        let idx = self.index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let chunks = self.chunks.lock().unwrap_or_else(|e| e.into_inner());
        if idx < chunks.len() {
            Ok(chunks[idx].clone())
        } else {
            // 返回静音块
            Ok(AudioChunk::new(vec![0.0; 480], 16000, 1, idx as u64, idx as u64 * 30))
        }
    }

    fn stop(&self) {}

    fn is_active(&self) -> bool {
        let idx = self.index.load(std::sync::atomic::Ordering::SeqCst);
        let len = self.chunks.lock().unwrap_or_else(|e| e.into_inner()).len();
        idx < len
    }
}
