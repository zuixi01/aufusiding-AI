//! 流式 HTTP STT 适配器
//!
//! 累积音频块，finalize 时批量发送到 API。
//! 使用 reqwest::Client（异步），匹配 StreamingSttEngine trait。

use std::sync::Mutex;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::streaming_traits::StreamingSttEngine;
use super::streaming_types::AudioChunk;

/// 流式 HTTP STT 配置
#[derive(Debug, Clone)]
pub struct HttpStreamingSttConfig {
    /// API 基础 URL
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 输出采样率（用于 WAV 编码）
    pub sample_rate: u32,
}

impl HttpStreamingSttConfig {
    pub fn new(api_key: &str) -> Self {
        Self {
            base_url: "https://api.stt-service.example/v1".to_string(),
            api_key: api_key.to_string(),
            sample_rate: 16000,
        }
    }
}

/// 流式 HTTP STT 引擎
pub struct HttpStreamingSttEngine {
    config: HttpStreamingSttConfig,
    client: Client,
    /// 累积的音频采样
    buffer: Mutex<Vec<f32>>,
    /// 已喂入的块数
    chunks_fed: Mutex<u64>,
}

/// API 转录响应
#[derive(Debug, Deserialize)]
struct TranscribeResponse {
    text: String,
}

impl HttpStreamingSttEngine {
    /// 创建流式 HTTP STT 引擎
    pub fn new(config: HttpStreamingSttConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            buffer: Mutex::new(Vec::new()),
            chunks_fed: Mutex::new(0),
        }
    }

    /// 创建带自定义 base URL 的引擎（用于测试）
    pub fn with_base_url(api_key: &str, base_url: &str) -> Self {
        let config = HttpStreamingSttConfig {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            sample_rate: 16000,
        };
        Self {
            config,
            client: Client::new(),
            buffer: Mutex::new(Vec::new()),
            chunks_fed: Mutex::new(0),
        }
    }

    /// 将 f32 采样编码为 WAV 字节
    fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
        let num_samples = samples.len();
        let data_size = (num_samples * 2) as u32;
        let file_size = 36 + data_size;

        let mut buf = Vec::with_capacity(44 + data_size as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // mono
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        for &sample in samples {
            let s = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            buf.extend_from_slice(&s.to_le_bytes());
        }
        buf
    }
}

#[async_trait]
impl StreamingSttEngine for HttpStreamingSttEngine {
    async fn feed_chunk(&self, chunk: &AudioChunk) -> anyhow::Result<Option<String>> {
        let mut buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buf.extend_from_slice(&chunk.samples);
        let mut count = self.chunks_fed.lock().unwrap_or_else(|e| e.into_inner());
        *count += 1;
        // 不在每个块上调用 API，仅累积
        Ok(None)
    }

    async fn finalize(&self) -> anyhow::Result<String> {
        let samples = {
            let buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
            buf.clone()
        };
        if samples.is_empty() {
            return Ok(String::new());
        }
        let wav_bytes = Self::encode_wav(&samples, self.config.sample_rate);
        let part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let resp = self
            .client
            .post(format!("{}/transcribe", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Streaming STT API 请求失败: {}", resp.status());
        }
        let body: TranscribeResponse = resp.json().await?;
        Ok(body.text)
    }

    async fn reset(&self) -> anyhow::Result<()> {
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        *self.chunks_fed.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn test_chunk(seq: u64) -> AudioChunk {
        AudioChunk::new(vec![0.1; 480], 16000, 1, seq, seq * 30)
    }

    #[tokio::test]
    async fn feed_chunk_accumulates_samples() {
        let engine = HttpStreamingSttEngine::with_base_url("key", "http://unused");
        engine.feed_chunk(&test_chunk(0)).await.unwrap();
        engine.feed_chunk(&test_chunk(1)).await.unwrap();
        let buf = engine.buffer.lock().unwrap();
        assert_eq!(buf.len(), 960);
    }

    #[tokio::test]
    async fn finalize_sends_accumulated_audio() {
        let server = MockServer::start_async().await;
        let mock = server.mock(|when, then| {
            when.method(POST).path("/transcribe");
            then.status(200)
                .json_body(serde_json::json!({"text": "hello streaming"}));
        });
        let engine = HttpStreamingSttEngine::with_base_url("key", &server.base_url());
        engine.feed_chunk(&test_chunk(0)).await.unwrap();
        let result = engine.finalize().await.unwrap();
        assert_eq!(result, "hello streaming");
        mock.assert();
    }

    #[tokio::test]
    async fn finalize_empty_buffer_returns_empty() {
        let engine = HttpStreamingSttEngine::with_base_url("key", "http://unused");
        let result = engine.finalize().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn reset_clears_buffer() {
        let engine = HttpStreamingSttEngine::with_base_url("key", "http://unused");
        engine.feed_chunk(&test_chunk(0)).await.unwrap();
        engine.reset().await.unwrap();
        let buf = engine.buffer.lock().unwrap();
        assert!(buf.is_empty());
    }

    #[tokio::test]
    async fn finalize_api_error_returns_err() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(POST).path("/transcribe");
            then.status(500);
        });
        let engine = HttpStreamingSttEngine::with_base_url("key", &server.base_url());
        engine.feed_chunk(&test_chunk(0)).await.unwrap();
        assert!(engine.finalize().await.is_err());
    }
}
