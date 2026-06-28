//! 语音处理管线
//!
//! 将 STT → 文本处理 → TTS 串联为完整的语音交互流程。

use super::traits::{SpeechToTextEngine, TextToSpeechEngine, WakewordEngine};
use super::types::AudioBuffer;

/// 语音管线
pub struct VoicePipeline {
    stt: Box<dyn SpeechToTextEngine>,
    tts: Box<dyn TextToSpeechEngine>,
    wakeword: Option<Box<dyn WakewordEngine>>,
}

impl VoicePipeline {
    /// 创建语音管线
    pub fn new(stt: Box<dyn SpeechToTextEngine>, tts: Box<dyn TextToSpeechEngine>) -> Self {
        Self {
            stt,
            tts,
            wakeword: None,
        }
    }

    /// 设置唤醒词引擎
    pub fn with_wakeword(mut self, wakeword: Box<dyn WakewordEngine>) -> Self {
        self.wakeword = Some(wakeword);
        self
    }

    /// 处理音频：STT → 文本处理回调 → TTS
    pub fn process<F>(&self, audio: &AudioBuffer, text_processor: F) -> anyhow::Result<AudioBuffer>
    where
        F: FnOnce(&str) -> String,
    {
        // 如果有唤醒词引擎，先检测
        if let Some(ref ww) = self.wakeword {
            if !ww.detect(audio)? {
                anyhow::bail!("未检测到唤醒词");
            }
        }

        // STT: 音频 → 文本
        let text = self.stt.transcribe(audio)?;

        // 文本处理
        let response = text_processor(&text);

        // TTS: 文本 → 音频
        self.tts.synthesize(&response)
    }

    /// 仅执行 STT
    pub fn transcribe(&self, audio: &AudioBuffer) -> anyhow::Result<String> {
        self.stt.transcribe(audio)
    }

    /// 仅执行 TTS
    pub fn synthesize(&self, text: &str) -> anyhow::Result<AudioBuffer> {
        self.tts.synthesize(text)
    }

    /// 检测唤醒词
    pub fn detect_wakeword(&self, audio: &AudioBuffer) -> anyhow::Result<bool> {
        match &self.wakeword {
            Some(ww) => ww.detect(audio),
            None => Ok(true), // 无唤醒词引擎时默认通过
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::mock::{MockSTT, MockTTS, MockWakeword};

    fn test_audio() -> AudioBuffer {
        AudioBuffer::new(vec![0.1; 16000], 16000, 1).unwrap()
    }

    fn make_pipeline() -> VoicePipeline {
        VoicePipeline::new(
            Box::new(MockSTT::new("你好")),
            Box::new(MockTTS::new(16000, 1)),
        )
    }

    #[test]
    fn full_pipeline_process() {
        let pipeline = make_pipeline();
        let result = pipeline
            .process(&test_audio(), |text| format!("回复: {text}"))
            .unwrap();
        assert!(!result.is_empty());
        assert_eq!(result.sample_rate, 16000);
    }

    #[test]
    fn pipeline_with_wakeword_detected() {
        let pipeline = make_pipeline().with_wakeword(Box::new(MockWakeword::new(true)));
        let result = pipeline.process(&test_audio(), |t| t.to_uppercase());
        assert!(result.is_ok());
    }

    #[test]
    fn pipeline_with_wakeword_not_detected() {
        let pipeline = make_pipeline().with_wakeword(Box::new(MockWakeword::new(false)));
        let result = pipeline.process(&test_audio(), |t| t.to_string());
        assert!(result.is_err());
    }

    #[test]
    fn pipeline_transcribe_only() {
        let pipeline = make_pipeline();
        let text = pipeline.transcribe(&test_audio()).unwrap();
        assert_eq!(text, "你好");
    }

    #[test]
    fn pipeline_synthesize_only() {
        let pipeline = make_pipeline();
        let audio = pipeline.synthesize("测试").unwrap();
        assert!(!audio.is_empty());
    }

    #[test]
    fn pipeline_no_wakeword_always_passes() {
        let pipeline = make_pipeline();
        assert!(pipeline.detect_wakeword(&test_audio()).unwrap());
    }
}
