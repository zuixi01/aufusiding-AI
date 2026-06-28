//! 流式语音引擎 trait
//!
//! 定义流式 STT、TTS 和 VAD 的抽象接口。

use async_trait::async_trait;

use super::streaming_types::{AudioChunk, VadDecision};

/// 流式 STT 引擎：逐块喂入音频，返回部分转录
#[async_trait]
pub trait StreamingSttEngine: Send + Sync {
    /// 喂入一个音频块，返回部分转录（如有）
    async fn feed_chunk(&self, chunk: &AudioChunk) -> anyhow::Result<Option<String>>;
    /// 结束输入，返回最终转录
    async fn finalize(&self) -> anyhow::Result<String>;
    /// 重置内部状态
    async fn reset(&self) -> anyhow::Result<()>;
}

/// 流式 TTS 引擎：文本输入，返回音频块序列
#[async_trait]
pub trait StreamingTtsEngine: Send + Sync {
    /// 将文本合成为音频块序列
    async fn synthesize_streamed(&self, text: &str) -> anyhow::Result<Vec<AudioChunk>>;
}

/// VAD 引擎：判定音频块是语音还是静音
pub trait VadEngine: Send + Sync {
    /// 分类单个音频块
    fn classify(&self, chunk: &AudioChunk) -> anyhow::Result<VadDecision>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证 trait 对象可以构造
    #[test]
    fn trait_objects_are_object_safe() {
        fn _assert_stt(_: &dyn StreamingSttEngine) {}
        fn _assert_tts(_: &dyn StreamingTtsEngine) {}
        fn _assert_vad(_: &dyn VadEngine) {}
    }
}
