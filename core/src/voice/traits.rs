//! 语音引擎 trait 定义

use super::types::AudioBuffer;

/// 语音转文字引擎
pub trait SpeechToTextEngine: Send + Sync {
    /// 转录音频为文本
    fn transcribe(&self, audio: &AudioBuffer) -> anyhow::Result<String>;
}

/// 文字转语音引擎
pub trait TextToSpeechEngine: Send + Sync {
    /// 合成文本为音频
    fn synthesize(&self, text: &str) -> anyhow::Result<AudioBuffer>;
}

/// 唤醒词检测引擎
pub trait WakewordEngine: Send + Sync {
    /// 检测音频中是否包含唤醒词
    fn detect(&self, audio: &AudioBuffer) -> anyhow::Result<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummySTT;
    impl SpeechToTextEngine for DummySTT {
        fn transcribe(&self, _audio: &AudioBuffer) -> anyhow::Result<String> {
            Ok("dummy".into())
        }
    }

    struct DummyTTS;
    impl TextToSpeechEngine for DummyTTS {
        fn synthesize(&self, _text: &str) -> anyhow::Result<AudioBuffer> {
            Ok(AudioBuffer::silence(0.1, 16000, 1))
        }
    }

    struct DummyWakeword;
    impl WakewordEngine for DummyWakeword {
        fn detect(&self, _audio: &AudioBuffer) -> anyhow::Result<bool> {
            Ok(false)
        }
    }

    #[test]
    fn trait_objects_can_be_created() {
        let _stt: Box<dyn SpeechToTextEngine> = Box::new(DummySTT);
        let _tts: Box<dyn TextToSpeechEngine> = Box::new(DummyTTS);
        let _ww: Box<dyn WakewordEngine> = Box::new(DummyWakeword);
    }
}
