//! Whisper HTTP STT 适配器
//!
//! 通过 HTTP 调用 Whisper API 实现语音转文字。
//! 使用 reqwest::blocking::Client（同步），匹配 SpeechToTextEngine trait。

use reqwest::blocking::Client;
use serde::Deserialize;

use super::traits::SpeechToTextEngine;
use super::types::AudioBuffer;

/// Whisper API 配置
#[derive(Debug, Clone)]
pub struct WhisperConfig {
    /// API 基础 URL
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 模型名称 (e.g. "whisper-1")
    pub model: String,
}

impl WhisperConfig {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }
}

/// Whisper HTTP STT 引擎
pub struct WhisperSttEngine {
    config: WhisperConfig,
    client: Client,
}

/// Whisper API 响应
#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
}

impl WhisperSttEngine {
    /// 创建 Whisper STT 引擎
    pub fn new(config: WhisperConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 创建带自定义 base URL 的引擎（用于测试）
    pub fn with_base_url(api_key: &str, model: &str, base_url: &str) -> Self {
        let config = WhisperConfig {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        };
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 将 AudioBuffer 编码为 WAV 字节
    fn encode_wav(audio: &AudioBuffer) -> Vec<u8> {
        let num_samples = audio.samples.len();
        let byte_rate = audio.sample_rate * audio.channels as u32 * 2;
        let data_size = (num_samples * 2) as u32;
        let file_size = 36 + data_size;

        let mut buf = Vec::with_capacity(44 + data_size as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&audio.channels.to_le_bytes());
        buf.extend_from_slice(&audio.sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&(audio.channels * 2).to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        for &sample in &audio.samples {
            let s = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            buf.extend_from_slice(&s.to_le_bytes());
        }
        buf
    }
}

impl SpeechToTextEngine for WhisperSttEngine {
    fn transcribe(&self, audio: &AudioBuffer) -> anyhow::Result<String> {
        if audio.is_empty() {
            anyhow::bail!("音频数据为空");
        }
        let wav_bytes = Self::encode_wav(audio);
        let part = reqwest::blocking::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;
        let form = reqwest::blocking::multipart::Form::new()
            .text("model", self.config.model.clone())
            .part("file", part);

        let resp = self
            .client
            .post(format!("{}/audio/transcriptions", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()?;

        if !resp.status().is_success() {
            anyhow::bail!("Whisper API 请求失败: {}", resp.status());
        }
        let body: WhisperResponse = resp.json()?;
        Ok(body.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn test_audio() -> AudioBuffer {
        let samples: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        AudioBuffer::new(samples, 16000, 1).unwrap()
    }

    #[test]
    fn transcribe_sends_multipart_post() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/audio/transcriptions");
            then.status(200)
                .json_body(serde_json::json!({"text": "hello world"}));
        });
        let engine = WhisperSttEngine::with_base_url("sk-test", "whisper-1", &server.base_url());
        let result = engine.transcribe(&test_audio()).unwrap();
        assert_eq!(result, "hello world");
        mock.assert();
    }

    #[test]
    fn transcribe_includes_bearer_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/audio/transcriptions")
                .header("Authorization", "Bearer sk-secret");
            then.status(200)
                .json_body(serde_json::json!({"text": "auth ok"}));
        });
        let engine = WhisperSttEngine::with_base_url("sk-secret", "whisper-1", &server.base_url());
        let result = engine.transcribe(&test_audio()).unwrap();
        assert_eq!(result, "auth ok");
        mock.assert();
    }

    #[test]
    fn transcribe_empty_audio_fails() {
        let engine = WhisperSttEngine::with_base_url("sk-test", "whisper-1", "http://unused");
        let empty = AudioBuffer::new(vec![], 16000, 1).unwrap();
        assert!(engine.transcribe(&empty).is_err());
    }

    #[test]
    fn transcribe_api_error_returns_err() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/audio/transcriptions");
            then.status(500);
        });
        let engine = WhisperSttEngine::with_base_url("sk-test", "whisper-1", &server.base_url());
        assert!(engine.transcribe(&test_audio()).is_err());
    }

    #[test]
    fn config_defaults_to_openai_url() {
        let config = WhisperConfig::new("sk-test", "whisper-1");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.model, "whisper-1");
    }
}
