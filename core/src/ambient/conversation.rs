//! 连续对话管理
//!
//! 管理连续对话会话：超时自动结束、上下文保持、轮数限制。

use chrono::{DateTime, Utc};
use std::time::Duration;
use uuid::Uuid;

use super::biometrics::SpeakerProfile;
use super::types::ConversationTurn;

/// 对话会话
#[derive(Debug, Clone)]
pub struct ConversationSession {
    pub id: Uuid,
    pub speaker: Option<SpeakerProfile>,
    pub started_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub turns: Vec<ConversationTurn>,
}

/// 连续对话管理器
pub struct ConversationManager {
    active_session: Option<ConversationSession>,
    /// 无语音超时（默认 30 秒）
    timeout: Duration,
    /// 最大连续对话轮数
    max_turns: usize,
}

impl ConversationManager {
    pub fn new(timeout: Duration, max_turns: usize) -> Self {
        Self {
            active_session: None,
            timeout,
            max_turns,
        }
    }

    /// 开始新会话
    pub fn start_session(&mut self, speaker: Option<&SpeakerProfile>) {
        let now = Utc::now();
        self.active_session = Some(ConversationSession {
            id: Uuid::new_v4(),
            speaker: speaker.cloned(),
            started_at: now,
            last_activity: now,
            turns: Vec::new(),
        });
    }

    /// 添加对话轮次
    pub fn add_turn(&mut self, user_text: &str, response: &str) {
        if let Some(session) = &mut self.active_session {
            session.turns.push(ConversationTurn::new(
                user_text.to_string(),
                response.to_string(),
            ));
            session.last_activity = Utc::now();
        }
    }

    /// 是否有活跃会话
    pub fn is_active(&self) -> bool {
        self.active_session.is_some()
    }

    /// 当前会话轮数
    pub fn turn_count(&self) -> usize {
        self.active_session
            .as_ref()
            .map(|s| s.turns.len())
            .unwrap_or(0)
    }

    /// 是否应该超时结束
    pub fn should_timeout(&self, now: DateTime<Utc>) -> bool {
        match &self.active_session {
            Some(session) => {
                let elapsed = now.signed_duration_since(session.last_activity);
                elapsed.num_milliseconds() >= self.timeout.as_millis() as i64
            }
            None => false,
        }
    }

    /// 是否达到最大轮数
    pub fn is_max_turns_reached(&self) -> bool {
        self.turn_count() >= self.max_turns
    }

    /// 结束会话，返回对话历史
    pub fn end_session(&mut self) -> Vec<ConversationTurn> {
        self.active_session
            .take()
            .map(|s| s.turns)
            .unwrap_or_default()
    }

    /// 获取活跃会话
    pub fn active_session(&self) -> Option<&ConversationSession> {
        self.active_session.as_ref()
    }
}
