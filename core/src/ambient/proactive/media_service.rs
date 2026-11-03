//! 媒体播放服务
//!
//! 统一管理音乐/视频/播客的播放控制，支持跨设备转移、断点恢复、
//! 多会话并发。通过 MediaProvider trait 抽象不同播放源。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── 核心类型 ────────────────────────────────────────────

/// 播放状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
    Buffering,
}

/// 曲目信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: u64,
    pub cover_url: Option<String>,
}

/// 媒体会话状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaState {
    pub session_id: Uuid,
    pub track: TrackInfo,
    pub position_secs: u64,
    pub status: PlaybackStatus,
    pub device_id: String,
    pub updated_at: DateTime<Utc>,
}

/// 队列中的曲目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub track: TrackInfo,
    pub index: usize,
}

// ─── MediaProvider trait ─────────────────────────────────

/// 媒体播放源 trait
#[async_trait]
pub trait MediaProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn play(&self, track: &TrackInfo, device_id: &str) -> anyhow::Result<()>;
    async fn pause(&self, device_id: &str) -> anyhow::Result<()>;
    async fn resume(&self, device_id: &str) -> anyhow::Result<()>;
    async fn stop(&self, device_id: &str) -> anyhow::Result<()>;
    async fn seek(&self, device_id: &str, position_secs: u64) -> anyhow::Result<()>;
    async fn get_position(&self, device_id: &str) -> anyhow::Result<u64>;
    async fn get_queue(&self, device_id: &str) -> anyhow::Result<Vec<QueueItem>>;
}

/// Mock 媒体播放源
pub struct MockMediaProvider {
    provider_name: String,
    positions: Mutex<HashMap<String, u64>>,
    statuses: Mutex<HashMap<String, PlaybackStatus>>,
}

impl MockMediaProvider {
    pub fn new(name: &str) -> Self {
        Self {
            provider_name: name.to_string(),
            positions: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl MediaProvider for MockMediaProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    async fn play(&self, _track: &TrackInfo, device_id: &str) -> anyhow::Result<()> {
        self.positions
            .lock()
            .map_err(|_| anyhow::anyhow!("positions lock poisoned"))?
            .insert(device_id.to_string(), 0);
        self.statuses
            .lock()
            .map_err(|_| anyhow::anyhow!("statuses lock poisoned"))?
            .insert(device_id.to_string(), PlaybackStatus::Playing);
        Ok(())
    }

    async fn pause(&self, device_id: &str) -> anyhow::Result<()> {
        self.statuses
            .lock()
            .map_err(|_| anyhow::anyhow!("statuses lock poisoned"))?
            .insert(device_id.to_string(), PlaybackStatus::Paused);
        Ok(())
    }

    async fn resume(&self, device_id: &str) -> anyhow::Result<()> {
        self.statuses
            .lock()
            .map_err(|_| anyhow::anyhow!("statuses lock poisoned"))?
            .insert(device_id.to_string(), PlaybackStatus::Playing);
        Ok(())
    }

    async fn stop(&self, device_id: &str) -> anyhow::Result<()> {
        self.statuses
            .lock()
            .map_err(|_| anyhow::anyhow!("statuses lock poisoned"))?
            .insert(device_id.to_string(), PlaybackStatus::Stopped);
        Ok(())
    }

    async fn seek(&self, device_id: &str, position_secs: u64) -> anyhow::Result<()> {
        self.positions
            .lock()
            .map_err(|_| anyhow::anyhow!("positions lock poisoned"))?
            .insert(device_id.to_string(), position_secs);
        Ok(())
    }

    async fn get_position(&self, device_id: &str) -> anyhow::Result<u64> {
        let positions = self
            .positions
            .lock()
            .map_err(|_| anyhow::anyhow!("positions lock poisoned"))?;
        Ok(*positions.get(device_id).unwrap_or(&0))
    }

    async fn get_queue(&self, _device_id: &str) -> anyhow::Result<Vec<QueueItem>> {
        Ok(Vec::new())
    }
}

// ─── MediaService ────────────────────────────────────────

/// 媒体播放服务
pub struct MediaService {
    providers: HashMap<String, Box<dyn MediaProvider>>,
    active_sessions: Mutex<HashMap<Uuid, MediaState>>,
    saved_positions: Mutex<HashMap<String, u64>>,
}

impl MediaService {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            active_sessions: Mutex::new(HashMap::new()),
            saved_positions: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_provider(&mut self, provider: Box<dyn MediaProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    pub async fn play(
        &self,
        provider_name: &str,
        track: TrackInfo,
        device_id: &str,
    ) -> anyhow::Result<Uuid> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("unknown provider: {}", provider_name))?;
        provider.play(&track, device_id).await?;

        let session_id = Uuid::new_v4();
        let state = MediaState {
            session_id,
            track,
            position_secs: 0,
            status: PlaybackStatus::Playing,
            device_id: device_id.to_string(),
            updated_at: Utc::now(),
        };
        self.active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("sessions lock poisoned"))?
            .insert(session_id, state);
        Ok(session_id)
    }

    pub async fn pause(&self, session_id: &Uuid) -> anyhow::Result<()> {
        let (provider_name, device_id) = self.session_info(session_id)?;
        if let Some(provider) = self.providers.get(&provider_name) {
            provider.pause(&device_id).await?;
        }
        let mut sessions = self
            .active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("sessions lock poisoned"))?;
        if let Some(s) = sessions.get_mut(session_id) {
            s.status = PlaybackStatus::Paused;
            s.updated_at = Utc::now();
        }
        Ok(())
    }

    pub async fn resume(&self, session_id: &Uuid) -> anyhow::Result<()> {
        let (provider_name, device_id) = self.session_info(session_id)?;
        if let Some(provider) = self.providers.get(&provider_name) {
            provider.resume(&device_id).await?;
        }
        let mut sessions = self
            .active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("sessions lock poisoned"))?;
        if let Some(s) = sessions.get_mut(session_id) {
            s.status = PlaybackStatus::Playing;
            s.updated_at = Utc::now();
        }
        Ok(())
    }

    pub async fn stop(&self, session_id: &Uuid) -> anyhow::Result<()> {
        let (provider_name, device_id) = self.session_info(session_id)?;
        if let Some(provider) = self.providers.get(&provider_name) {
            provider.stop(&device_id).await?;
        }
        // save position before removing
        let mut sessions = self
            .active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("sessions lock poisoned"))?;
        if let Some(s) = sessions.remove(session_id) {
            let mut saved = self
                .saved_positions
                .lock()
                .map_err(|_| anyhow::anyhow!("saved_positions lock poisoned"))?;
            saved.insert(s.track.title.clone(), s.position_secs);
        }
        Ok(())
    }

    pub async fn seek(&self, session_id: &Uuid, position_secs: u64) -> anyhow::Result<()> {
        let (provider_name, device_id) = self.session_info(session_id)?;
        if let Some(provider) = self.providers.get(&provider_name) {
            provider.seek(&device_id, position_secs).await?;
        }
        let mut sessions = self
            .active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("sessions lock poisoned"))?;
        if let Some(s) = sessions.get_mut(session_id) {
            s.position_secs = position_secs;
            s.updated_at = Utc::now();
        }
        Ok(())
    }

    /// 跨设备转移：保存位置 → 停旧设备 → 新设备恢复
    pub async fn transfer_to_device(
        &self,
        session_id: &Uuid,
        target_device: &str,
    ) -> anyhow::Result<()> {
        let (provider_name, old_device) = self.session_info(session_id)?;
        let provider = self
            .providers
            .get(&provider_name)
            .ok_or_else(|| anyhow::anyhow!("unknown provider: {}", provider_name))?;

        // get current position
        let position = provider.get_position(&old_device).await.unwrap_or(0);

        // stop on old device
        provider.stop(&old_device).await?;

        // get track info
        let track = {
            let sessions = self
                .active_sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("sessions lock poisoned"))?;
            sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?
                .track
                .clone()
        };

        // play on new device
        provider.play(&track, target_device).await?;
        provider.seek(target_device, position).await?;

        let mut sessions = self
            .active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("sessions lock poisoned"))?;
        if let Some(s) = sessions.get_mut(session_id) {
            s.device_id = target_device.to_string();
            s.position_secs = position;
            s.status = PlaybackStatus::Playing;
            s.updated_at = Utc::now();
        }
        Ok(())
    }

    pub fn get_saved_position(&self, track_title: &str) -> Option<u64> {
        self.saved_positions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(track_title)
            .copied()
    }

    pub fn get_session(&self, session_id: &Uuid) -> Option<MediaState> {
        self.active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(session_id)
            .cloned()
    }

    pub fn active_session_count(&self) -> usize {
        self.active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// 语音摘要
    pub fn voice_summary(&self) -> String {
        let sessions = self
            .active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let playing: Vec<&MediaState> = sessions
            .values()
            .filter(|s| s.status == PlaybackStatus::Playing)
            .collect();

        if playing.is_empty() {
            return "当前没有正在播放的媒体。".into();
        }

        let s = &playing[0];
        let hours = s.position_secs / 3600;
        let mins = (s.position_secs % 3600) / 60;
        let secs = s.position_secs % 60;
        let pos = if hours > 0 {
            format!("{}:{:02}:{:02}", hours, mins, secs)
        } else {
            format!("{}:{:02}", mins, secs)
        };
        format!("正在播放《{}》，进度 {}。", s.track.title, pos)
    }

    fn session_info(&self, session_id: &Uuid) -> anyhow::Result<(String, String)> {
        let sessions = self
            .active_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("sessions lock poisoned"))?;
        let state = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;
        // find which provider owns this - for simplicity use first provider
        let provider_name = self
            .providers
            .keys()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no providers registered"))?
            .clone();
        Ok((provider_name, state.device_id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_track() -> TrackInfo {
        TrackInfo {
            title: "奥本海默".into(),
            artist: "Ludwig Göransson".into(),
            album: "Oppenheimer OST".into(),
            duration_secs: 7200,
            cover_url: None,
        }
    }

    fn test_track2() -> TrackInfo {
        TrackInfo {
            title: "夜曲".into(),
            artist: "周杰伦".into(),
            album: "十一月的萧邦".into(),
            duration_secs: 265,
            cover_url: None,
        }
    }

    fn setup() -> MediaService {
        let mut svc = MediaService::new();
        svc.register_provider(Box::new(MockMediaProvider::new("mock")));
        svc
    }

    #[tokio::test]
    async fn play_creates_session() {
        let svc = setup();
        let sid = svc.play("mock", test_track(), "speaker-001").await.unwrap();
        assert_eq!(svc.active_session_count(), 1);
        let state = svc.get_session(&sid).unwrap();
        assert_eq!(state.status, PlaybackStatus::Playing);
        assert_eq!(state.device_id, "speaker-001");
    }

    #[tokio::test]
    async fn play_unknown_provider() {
        let svc = setup();
        assert!(svc.play("nope", test_track(), "speaker-001").await.is_err());
    }

    #[tokio::test]
    async fn pause_and_resume() {
        let svc = setup();
        let sid = svc.play("mock", test_track(), "speaker-001").await.unwrap();
        svc.pause(&sid).await.unwrap();
        assert_eq!(
            svc.get_session(&sid).unwrap().status,
            PlaybackStatus::Paused
        );
        svc.resume(&sid).await.unwrap();
        assert_eq!(
            svc.get_session(&sid).unwrap().status,
            PlaybackStatus::Playing
        );
    }

    #[tokio::test]
    async fn stop_removes_session_and_saves_position() {
        let svc = setup();
        let sid = svc.play("mock", test_track(), "speaker-001").await.unwrap();
        svc.seek(&sid, 5025).await.unwrap();
        svc.stop(&sid).await.unwrap();
        assert_eq!(svc.active_session_count(), 0);
        assert_eq!(svc.get_saved_position("奥本海默"), Some(5025));
    }

    #[tokio::test]
    async fn seek_updates_position() {
        let svc = setup();
        let sid = svc.play("mock", test_track(), "speaker-001").await.unwrap();
        svc.seek(&sid, 3600).await.unwrap();
        let state = svc.get_session(&sid).unwrap();
        assert_eq!(state.position_secs, 3600);
    }

    #[tokio::test]
    async fn transfer_to_device() {
        let svc = setup();
        let sid = svc.play("mock", test_track(), "speaker-001").await.unwrap();
        svc.seek(&sid, 1000).await.unwrap();
        svc.transfer_to_device(&sid, "tv-001").await.unwrap();
        let state = svc.get_session(&sid).unwrap();
        assert_eq!(state.device_id, "tv-001");
        assert_eq!(state.status, PlaybackStatus::Playing);
    }

    #[tokio::test]
    async fn transfer_unknown_session() {
        let svc = setup();
        assert!(svc
            .transfer_to_device(&Uuid::new_v4(), "tv-001")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn multiple_concurrent_sessions() {
        let svc = setup();
        let s1 = svc.play("mock", test_track(), "speaker-001").await.unwrap();
        let s2 = svc
            .play("mock", test_track2(), "speaker-002")
            .await
            .unwrap();
        assert_eq!(svc.active_session_count(), 2);
        svc.stop(&s1).await.unwrap();
        assert_eq!(svc.active_session_count(), 1);
        assert!(svc.get_session(&s2).is_some());
    }

    #[tokio::test]
    async fn voice_summary_playing() {
        let svc = setup();
        let sid = svc.play("mock", test_track(), "speaker-001").await.unwrap();
        svc.seek(&sid, 5025).await.unwrap();
        let summary = svc.voice_summary();
        assert!(summary.contains("奥本海默"));
        assert!(summary.contains("1:23:45"));
    }

    #[test]
    fn voice_summary_nothing_playing() {
        let svc = setup();
        assert!(svc.voice_summary().contains("没有正在播放"));
    }

    #[tokio::test]
    async fn pause_unknown_session() {
        let svc = setup();
        assert!(svc.pause(&Uuid::new_v4()).await.is_err());
    }

    #[tokio::test]
    async fn get_saved_position_none() {
        let svc = setup();
        assert_eq!(svc.get_saved_position("nonexistent"), None);
    }

    #[test]
    fn playback_status_serde_roundtrip() {
        let s = PlaybackStatus::Buffering;
        let json = serde_json::to_string(&s).unwrap();
        let back: PlaybackStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn track_info_serde_roundtrip() {
        let t = test_track();
        let json = serde_json::to_string(&t).unwrap();
        let back: TrackInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "奥本海默");
    }

    #[test]
    fn mock_provider_name() {
        let p = MockMediaProvider::new("spotify");
        assert_eq!(p.name(), "spotify");
    }
}
