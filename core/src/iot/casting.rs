//! 投屏服务
//!
//! DLNA/AirPlay/Miracast 投屏统一接口。

use serde::{Deserialize, Serialize};

/// 投屏协议
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastProtocol {
    Dlna,
    AirPlay,
    Miracast,
    ChromeCast,
}

/// 投屏目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastTarget {
    pub device_id: String,
    pub device_name: String,
    pub protocol: CastProtocol,
    pub ip: String,
    pub port: u16,
}

/// 投屏会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastSessionState {
    Idle,
    Connecting,
    Casting,
    Paused,
    Error,
}

/// 投屏服务
pub struct CastingService {
    active_sessions: std::sync::Mutex<Vec<CastSession>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CastSession {
    id: uuid::Uuid,
    target: CastTarget,
    state: CastSessionState,
    content_url: String,
}

impl CastingService {
    pub fn new() -> Self {
        Self {
            active_sessions: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 开始投屏
    pub async fn cast(&self, target: &CastTarget, content_url: &str) -> anyhow::Result<uuid::Uuid> {
        let session_id = uuid::Uuid::new_v4();
        let session = CastSession {
            id: session_id,
            target: target.clone(),
            state: CastSessionState::Casting,
            content_url: content_url.to_string(),
        };

        tracing::info!(
            target_device = %target.device_name,
            protocol = ?target.protocol,
            "开始投屏"
        );

        self.active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(session);
        Ok(session_id)
    }

    /// 停止投屏
    pub fn stop(&self, session_id: uuid::Uuid) -> anyhow::Result<()> {
        let mut sessions = self
            .active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) {
            session.state = CastSessionState::Idle;
            Ok(())
        } else {
            anyhow::bail!("投屏会话不存在: {}", session_id)
        }
    }

    /// 暂停投屏
    pub fn pause(&self, session_id: uuid::Uuid) -> anyhow::Result<()> {
        let mut sessions = self
            .active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) {
            session.state = CastSessionState::Paused;
            Ok(())
        } else {
            anyhow::bail!("投屏会话不存在: {}", session_id)
        }
    }

    /// 活跃投屏数
    pub fn active_count(&self) -> usize {
        self.active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|s| {
                matches!(
                    s.state,
                    CastSessionState::Casting | CastSessionState::Paused
                )
            })
            .count()
    }

    /// 获取会话状态
    pub fn session_state(&self, session_id: uuid::Uuid) -> Option<CastSessionState> {
        self.active_sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.state)
    }
}
