//! 流式语音 Mock 实现
//!
//! 用于测试的 MockStreamingStt 和 MockStreamingTts。

use std::sync::Mutex;

use async_trait::async_trait;

use super::streaming_traits::{StreamingSttEngine, StreamingTtsEngine};
use super::streaming_types::AudioChunk;

/// Mock 流式 STT
pub struct MockStreamingStt {
    chunks_fed: Mutex<u64>,
    final_text: String,
    partial_interval: u64,
}

impl MockStreamingStt {
    /// 创建 Mock STT
    /// - `final_text`: finalize 时返回的文本
    /// - `partial_interval`: 每隔多少块返回一次部分转录
    pub fn new(final_text: impl Into<String>, partial_interval: u64) -> Self {
        Self {
            chunks_fed: Mutex::new(0),
            final_text: final_text.into(),
            partial_interval: partial_interval.max(1),
        }
    }

    /// 已喂入的块数
    pub fn chunks_fed(&self) -> u64 {
        *self.chunks_fed.lock().unwrap()
    }
}

#[async_trait]
impl StreamingSttEngine for MockStreamingStt {
    async fn feed_chunk(&self, _chunk: &AudioChunk) -> anyhow::Result<Option<String>> {
        let mut count = self.chunks_fed.lock().unwrap();
        *count += 1;
        if (*count).is_multiple_of(self.partial_interval) {
            let progress = (*count as f32 / 10.0).min(1.0);
            let partial_len = (self.final_text.chars().count() as f32 * progress) as usize;
            let partial: String = self.final_text.chars().take(partial_len.max(1)).collect();
            Ok(Some(partial))
        } else {
            Ok(None)
        }
    }

    async fn finalize(&self) -> anyhow::Result<String> {
        Ok(self.final_text.clone())
    }

    async fn reset(&self) -> anyhow::Result<()> {
        *self.chunks_fed.lock().unwrap() = 0;
        Ok(())
    }
}

/// Mock 流式 TTS
pub struct MockStreamingTts {
    sample_rate: u32,
    channels: u16,
}

impl MockStreamingTts {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }
}

#[async_trait]
impl StreamingTtsEngine for MockStreamingTts {
    async fn synthesize_streamed(&self, text: &str) -> anyhow::Result<Vec<AudioChunk>> {
        // 每个字符生成一个 30ms 的静音块
        let samples_per_chunk = (self.sample_rate as usize * self.channels as usize * 30) / 1000;
        let chunks: Vec<AudioChunk> = text
            .chars()
            .enumerate()
            .map(|(i, _)| {
                AudioChunk::new(
                    vec![0.0; samples_per_chunk],
                    self.sample_rate,
                    self.channels,
                    i as u64,
                    i as u64 * 30,
                )
            })
            .collect();
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_stt_accumulates_and_partial() {
        let stt = MockStreamingStt::new("你好世界", 3);
        let chunk = AudioChunk::new(vec![0.1; 480], 16000, 1, 0, 0);

        assert!(stt.feed_chunk(&chunk).await.unwrap().is_none()); // 1
        assert!(stt.feed_chunk(&chunk).await.unwrap().is_none()); // 2
        let partial = stt.feed_chunk(&chunk).await.unwrap(); // 3 → 部分转录
        assert!(partial.is_some());
    }

    #[tokio::test]
    async fn mock_stt_finalize() {
        let stt = MockStreamingStt::new("测试文本", 3);
        let text = stt.finalize().await.unwrap();
        assert_eq!(text, "测试文本");
    }

    #[tokio::test]
    async fn mock_stt_reset() {
        let stt = MockStreamingStt::new("hi", 1);
        let chunk = AudioChunk::new(vec![0.1; 480], 16000, 1, 0, 0);
        stt.feed_chunk(&chunk).await.unwrap();
        assert_eq!(stt.chunks_fed(), 1);
        stt.reset().await.unwrap();
        assert_eq!(stt.chunks_fed(), 0);
    }

    #[tokio::test]
    async fn mock_tts_generates_chunks() {
        let tts = MockStreamingTts::new(16000, 1);
        let chunks = tts.synthesize_streamed("abc").await.unwrap();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].duration_ms(), 30);
        assert_eq!(chunks[1].sequence_id, 1);
    }
}
