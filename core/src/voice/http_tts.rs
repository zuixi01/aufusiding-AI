//! HTTP TTS 适配器
//!
//! 通过 HTTP 调用 TTS API 实现文字转语音。
//! 使用 reqwest::blocking::Client（同步），匹配 TextToSpeechEngine trait。

use reqwest::blocking::Client;

use super::traits::TextToSpeechEngine;
use super::types::AudioBuffer;

/// HTTP TTS 配置
#[derive(Debug, Clone)]
pub struct HttpTtsConfig {
    /// API 基础 URL
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 输出采样率
    pub sample_rate: u32,
    /// 输出声道数
    pub channels: u16,
}

impl HttpTtsConfig {
    pub fn new(api_key: &str) -> Self {
        Self {
            base_url: "https://api.tts-service.example/v1".to_string(),
            api_key: api_key.to_string(),
            sample_rate: 16000,
            channels: 1,
        }
    }
}

/// HTTP TTS 引擎
pub struct HttpTtsEngine {
    config: HttpTtsConfig,
    client: Client,
}

impl HttpTtsEngine {
    /// 创建 HTTP TTS 引擎
    pub fn new(config: HttpTtsConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 创建带自定义 base URL 的引擎（用于测试）
    pub fn with_base_url(api_key: &str, base_url: &str) -> Self {
        let config = HttpTtsConfig {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            sample_rate: 16000,
            channels: 1,
        };
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 将原始 PCM i16 LE 字节解码为 f32 采样
    fn decode_pcm_i16(bytes: &[u8]) -> Vec<f32> {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| {
                let sample = i16::from_le_bytes(*chunk);
                sample as f32 / 32768.0
            })
            .collect()
    }
}

impl TextToSpeechEngine for HttpTtsEngine {
    fn synthesize(&self, text: &str) -> anyhow::Result<AudioBuffer> {
        if text.is_empty() {
            anyhow::bail!("合成文本不能为空");
        }
        let body = serde_json::json!({
            "text": text,
        });
        let resp = self
            .client
            .post(format!("{}/synthesize", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()?;

        if !resp.status().is_success() {
            anyhow::bail!("TTS API 请求失败: {}", resp.status());
        }
        let raw_bytes = resp.bytes()?;
        let samples = Self::decode_pcm_i16(&raw_bytes);
        AudioBuffer::new(samples, self.config.sample_rate, self.config.channels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    /// 生成 PCM i16 LE 字节（静音）
    fn silent_pcm_bytes(num_samples: usize) -> Vec<u8> {
        vec![0u8; num_samples * 2]
    }

    /// 生成非零 PCM i16 LE 字节
    fn tone_pcm_bytes(num_samples: usize) -> Vec<u8> {
        (0..num_samples)
            .flat_map(|i| {
                let val = ((i as f32 * 0.1).sin() * 16000.0) as i16;
                val.to_le_bytes().to_vec()
            })
            .collect()
    }

    #[test]
    fn synthesize_returns_audio_buffer() {
        let server = MockServer::start();
        let pcm = tone_pcm_bytes(1600);
        server.mock(|when, then| {
            when.method(POST).path("/synthesize");
            then.status(200).body(&pcm);
        });
        let engine = HttpTtsEngine::with_base_url("key-test", &server.base_url());
        let buf = engine.synthesize("hello").unwrap();
        assert_eq!(buf.samples.len(), 1600);
        assert_eq!(buf.sample_rate, 16000);
    }

    #[test]
    fn synthesize_includes_bearer_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/synthesize")
                .header("Authorization", "Bearer my-key");
            then.status(200).body(silent_pcm_bytes(800));
        });
        let engine = HttpTtsEngine::with_base_url("my-key", &server.base_url());
        engine.synthesize("test").unwrap();
        mock.assert();
    }

    #[test]
    fn synthesize_empty_text_fails() {
        let engine = HttpTtsEngine::with_base_url("key", "http://unused");
        assert!(engine.synthesize("").is_err());
    }

    #[test]
    fn decode_pcm_ignores_incomplete_trailing_sample() {
        let mut bytes = 16_384_i16.to_le_bytes().to_vec();
        bytes.push(0xff);
        assert_eq!(HttpTtsEngine::decode_pcm_i16(&bytes), vec![0.5]);
        assert!(HttpTtsEngine::decode_pcm_i16(&bytes[..1]).is_empty());
    }

    #[test]
    fn synthesize_api_error_returns_err() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/synthesize");
            then.status(503);
        });
        let engine = HttpTtsEngine::with_base_url("key", &server.base_url());
        assert!(engine.synthesize("hello").is_err());
    }

    #[test]
    fn config_defaults() {
        let config = HttpTtsConfig::new("key-123");
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
        assert!(!config.base_url.is_empty());
    }
}
