//! Mock 语音引擎实现

use super::traits::{SpeechToTextEngine, TextToSpeechEngine, WakewordEngine};
use super::types::AudioBuffer;

/// Mock STT 引擎：返回可配置的固定文本
pub struct MockSTT {
    response_text: String,
}

impl MockSTT {
    pub fn new(response_text: &str) -> Self {
        Self {
            response_text: response_text.to_string(),
        }
    }
}

impl SpeechToTextEngine for MockSTT {
    fn transcribe(&self, audio: &AudioBuffer) -> anyhow::Result<String> {
        if audio.is_empty() {
            anyhow::bail!("音频数据为空");
        }
        Ok(self.response_text.clone())
    }
}

/// Mock TTS 引擎：返回静音 AudioBuffer
pub struct MockTTS {
    sample_rate: u32,
    channels: u16,
}

impl MockTTS {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }
}

impl TextToSpeechEngine for MockTTS {
    fn synthesize(&self, text: &str) -> anyhow::Result<AudioBuffer> {
        if text.is_empty() {
            anyhow::bail!("文本为空");
        }
        // 根据文本长度生成对应时长的静音
        let duration = text.len() as f64 * 0.05;
        Ok(AudioBuffer::silence(
            duration,
            self.sample_rate,
            self.channels,
        ))
    }
}

/// Mock 唤醒词引擎：可配置触发条件
pub struct MockWakeword {
    trigger: bool,
}

impl MockWakeword {
    pub fn new(trigger: bool) -> Self {
        Self { trigger }
    }
}

impl WakewordEngine for MockWakeword {
    fn detect(&self, audio: &AudioBuffer) -> anyhow::Result<bool> {
        if audio.is_empty() {
            anyhow::bail!("音频数据为空");
        }
        Ok(self.trigger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_audio() -> AudioBuffer {
        AudioBuffer::new(vec![0.1; 16000], 16000, 1).unwrap()
    }

    #[test]
    fn mock_stt_returns_configured_text() {
        let stt = MockSTT::new("你好世界");
        let result = stt.transcribe(&test_audio()).unwrap();
        assert_eq!(result, "你好世界");
    }

    #[test]
    fn mock_stt_empty_audio_fails() {
        let stt = MockSTT::new("test");
        let empty = AudioBuffer::new(vec![], 16000, 1).unwrap();
        assert!(stt.transcribe(&empty).is_err());
    }

    #[test]
    fn mock_tts_returns_silence() {
        let tts = MockTTS::new(16000, 1);
        let buf = tts.synthesize("hello").unwrap();
        assert!(!buf.is_empty());
        assert!(buf.samples.iter().all(|&s| s == 0.0));
        assert_eq!(buf.sample_rate, 16000);
    }

    #[test]
    fn mock_tts_empty_text_fails() {
        let tts = MockTTS::new(16000, 1);
        assert!(tts.synthesize("").is_err());
    }

    #[test]
    fn mock_tts_duration_scales_with_text() {
        let tts = MockTTS::new(16000, 1);
        let short = tts.synthesize("hi").unwrap();
        let long = tts.synthesize("hello world").unwrap();
        assert!(long.samples.len() > short.samples.len());
    }

    #[test]
    fn mock_wakeword_trigger_true() {
        let ww = MockWakeword::new(true);
        assert!(ww.detect(&test_audio()).unwrap());
    }

    #[test]
    fn mock_wakeword_trigger_false() {
        let ww = MockWakeword::new(false);
        assert!(!ww.detect(&test_audio()).unwrap());
    }

    #[test]
    fn mock_wakeword_empty_audio_fails() {
        let ww = MockWakeword::new(true);
        let empty = AudioBuffer::new(vec![], 16000, 1).unwrap();
        assert!(ww.detect(&empty).is_err());
    }
}
