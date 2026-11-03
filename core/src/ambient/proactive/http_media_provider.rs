//! HTTP 媒体播放提供者
//!
//! 通过 Sonos/通用 HTTP 风格 API 实现 MediaProvider trait。
//! 使用 reqwest::Client + 可配置 base_url，便于 httpmock 测试。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::media_service::{MediaProvider, QueueItem, TrackInfo};

// ─── API 类型 ────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct PlayRequest {
    title: String,
    artist: String,
    album: String,
    duration_secs: u64,
    device_id: String,
}

#[derive(Debug, Serialize)]
struct SeekRequest {
    position_secs: u64,
}

#[derive(Debug, Deserialize)]
struct PositionResponse {
    position_secs: u64,
}

#[derive(Debug, Deserialize)]
struct QueueResponse {
    items: Vec<QueueItemResponse>,
}

#[derive(Debug, Deserialize)]
struct QueueItemResponse {
    title: String,
    artist: String,
    album: String,
    duration_secs: u64,
    index: usize,
    cover_url: Option<String>,
}

// ─── HttpMediaProvider ──────────────────────────────────

/// HTTP 媒体播放提供者（Sonos/通用 HTTP 风格）
pub struct HttpMediaProvider {
    provider_name: String,
    client: Client,
    base_url: String,
}

impl HttpMediaProvider {
    pub fn new(provider_name: &str, base_url: &str) -> Self {
        Self {
            provider_name: provider_name.to_string(),
            client: Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// 创建带自定义 base URL 的实例（用于测试）
    pub fn with_base_url(provider_name: &str, base_url: &str) -> Self {
        Self::new(provider_name, base_url)
    }
}

#[async_trait]
impl MediaProvider for HttpMediaProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    async fn play(&self, track: &TrackInfo, device_id: &str) -> anyhow::Result<()> {
        let req = PlayRequest {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration_secs: track.duration_secs,
            device_id: device_id.to_string(),
        };
        let resp = self
            .client
            .post(format!("{}/playback/play", self.base_url))
            .json(&req)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("play failed: {}", resp.status());
        }
        Ok(())
    }

    async fn pause(&self, device_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/playback/pause", self.base_url))
            .query(&[("device_id", device_id)])
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("pause failed: {}", resp.status());
        }
        Ok(())
    }

    async fn resume(&self, device_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/playback/resume", self.base_url))
            .query(&[("device_id", device_id)])
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("resume failed: {}", resp.status());
        }
        Ok(())
    }

    async fn stop(&self, device_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{}/playback/stop", self.base_url))
            .query(&[("device_id", device_id)])
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("stop failed: {}", resp.status());
        }
        Ok(())
    }

    async fn seek(&self, device_id: &str, position_secs: u64) -> anyhow::Result<()> {
        let req = SeekRequest { position_secs };
        let resp = self
            .client
            .post(format!("{}/playback/seek", self.base_url))
            .query(&[("device_id", device_id)])
            .json(&req)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("seek failed: {}", resp.status());
        }
        Ok(())
    }

    async fn get_position(&self, device_id: &str) -> anyhow::Result<u64> {
        let resp = self
            .client
            .get(format!("{}/playback/position", self.base_url))
            .query(&[("device_id", device_id)])
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("get_position failed: {}", resp.status());
        }
        let pos: PositionResponse = resp.json().await?;
        Ok(pos.position_secs)
    }

    async fn get_queue(&self, device_id: &str) -> anyhow::Result<Vec<QueueItem>> {
        let resp = self
            .client
            .get(format!("{}/playback/queue", self.base_url))
            .query(&[("device_id", device_id)])
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("get_queue failed: {}", resp.status());
        }
        let queue: QueueResponse = resp.json().await?;
        Ok(queue
            .items
            .into_iter()
            .map(|item| QueueItem {
                track: TrackInfo {
                    title: item.title,
                    artist: item.artist,
                    album: item.album,
                    duration_secs: item.duration_secs,
                    cover_url: item.cover_url,
                },
                index: item.index,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn test_track() -> TrackInfo {
        TrackInfo {
            title: "Test Song".into(),
            artist: "Test Artist".into(),
            album: "Test Album".into(),
            duration_secs: 240,
            cover_url: None,
        }
    }

    #[tokio::test]
    async fn play_posts_to_playback_play() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/playback/play");
            then.status(200);
        });
        let provider = HttpMediaProvider::with_base_url("sonos", &server.base_url());
        provider.play(&test_track(), "speaker-1").await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn pause_posts_with_device_id() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/playback/pause")
                .query_param("device_id", "speaker-1");
            then.status(200);
        });
        let provider = HttpMediaProvider::with_base_url("sonos", &server.base_url());
        provider.pause("speaker-1").await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn resume_posts_to_playback_resume() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/playback/resume");
            then.status(200);
        });
        let provider = HttpMediaProvider::with_base_url("sonos", &server.base_url());
        provider.resume("speaker-1").await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn stop_posts_to_playback_stop() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/playback/stop");
            then.status(200);
        });
        let provider = HttpMediaProvider::with_base_url("sonos", &server.base_url());
        provider.stop("speaker-1").await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn seek_sends_position() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/playback/seek");
            then.status(200);
        });
        let provider = HttpMediaProvider::with_base_url("sonos", &server.base_url());
        provider.seek("speaker-1", 120).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn get_position_returns_secs() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/playback/position");
            then.status(200)
                .json_body(serde_json::json!({"position_secs": 42}));
        });
        let provider = HttpMediaProvider::with_base_url("sonos", &server.base_url());
        let pos = provider.get_position("speaker-1").await.unwrap();
        assert_eq!(pos, 42);
    }

    #[tokio::test]
    async fn get_queue_returns_items() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/playback/queue");
            then.status(200).json_body(serde_json::json!({
                "items": [{
                    "title": "Song A",
                    "artist": "Artist A",
                    "album": "Album A",
                    "duration_secs": 180,
                    "index": 0,
                    "cover_url": null
                }]
            }));
        });
        let provider = HttpMediaProvider::with_base_url("sonos", &server.base_url());
        let queue = provider.get_queue("speaker-1").await.unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].track.title, "Song A");
        assert_eq!(queue[0].index, 0);
    }
}
