//! 流式 HTTP TTS 引擎
//!
//! 发送文本到 TTS API，返回分块音频。

use async_trait::async_trait;
use reqwest::Client;

use crate::voice::streaming_traits::StreamingTtsEngine;
use crate::voice::streaming_types::AudioChunk;

/// HTTP 流式 TTS 引擎
pub struct HttpStreamingTtsEngine {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl HttpStreamingTtsEngine {
    pub fn new(base_url: &str, api_key: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    pub fn with_base_url(base_url: &str, api_key: &str) -> Self {
        Self::new(base_url, api_key, "tts-1")
    }
}

#[async_trait]
impl StreamingTtsEngine for HttpStreamingTtsEngine {
    async fn synthesize_streamed(&self, text: &str) -> anyhow::Result<Vec<AudioChunk>> {
        let body = serde_json::json!({
            "model": self.model,
            "input": text,
            "response_format": "pcm",
        });

        let resp = self
            .client
            .post(format!("{}/v1/audio/speech", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("TTS streaming API 失败: {}", resp.status());
        }

        let bytes = resp.bytes().await?;
        let samples: Vec<f32> = bytes
            .chunks(2)
            .map(|chunk| {
                if chunk.len() == 2 {
                    let val = i16::from_le_bytes([chunk[0], chunk[1]]);
                    val as f32 / 32768.0
                } else {
                    0.0
                }
            })
            .collect();

        let chunk_size = 480;
        let mut chunks = Vec::new();
        for (i, chunk_samples) in samples.chunks(chunk_size).enumerate() {
            chunks.push(AudioChunk::new(
                chunk_samples.to_vec(),
                16000,
                1,
                i as u64,
                i as u64 * 30,
            ));
        }

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn synthesize_streamed_calls_api() {
        let server = MockServer::start();
        let pcm_data: Vec<u8> = vec![0x00, 0x10, 0x00, 0x20, 0x00, 0x30, 0x00, 0x40];
        server.mock(|when, then| {
            when.method(POST).path("/v1/audio/speech");
            then.status(200).body(pcm_data);
        });
        let engine = HttpStreamingTtsEngine::with_base_url(&server.base_url(), "test-key");
        let chunks = engine.synthesize_streamed("hello").await.unwrap();
        assert!(!chunks.is_empty());
    }

    #[tokio::test]
    async fn synthesize_streamed_error_handling() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/audio/speech");
            then.status(500);
        });
        let engine = HttpStreamingTtsEngine::with_base_url(&server.base_url(), "key");
        assert!(engine.synthesize_streamed("test").await.is_err());
    }

    #[tokio::test]
    async fn synthesize_includes_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/audio/speech")
                .header("Authorization", "Bearer secret-key");
            then.status(200).body(vec![0x00, 0x10]);
        });
        let engine = HttpStreamingTtsEngine::with_base_url(&server.base_url(), "secret-key");
        engine.synthesize_streamed("hi").await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn empty_response_returns_empty_chunks() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/audio/speech");
            then.status(200).body(Vec::<u8>::new());
        });
        let engine = HttpStreamingTtsEngine::with_base_url(&server.base_url(), "key");
        let chunks = engine.synthesize_streamed("").await.unwrap();
        assert!(chunks.is_empty());
    }
}
