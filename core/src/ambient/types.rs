//! 环境计算类型定义
//!
//! 状态机、监听模式、对话轮次、分发结果等核心类型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 环境守护状态机
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbientState {
    /// VAD 监听中，无语音活动
    Idle,
    /// 唤醒词命中，验证中
    WakewordDetected,
    /// 主动监听用户命令
    Listening,
    /// STT 完成，分发执行中
    Processing,
    /// TTS 播放中
    Speaking,
    /// 连续对话模式（无需重新唤醒）
    Conversation,
}

impl AmbientState {
    /// 是否处于活跃交互状态
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// 监听模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListeningMode {
    /// 每次需要唤醒词触发
    WakewordRequired,
    /// 连续对话模式，无需重新触发
    Continuous,
    /// 手动按键激活
    PushToTalk,
}

/// 对话轮次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    /// 轮次 ID
    pub id: Uuid,
    /// 用户输入文本
    pub user_text: String,
    /// 系统响应文本
    pub response_text: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 说话人 ID（Phase 2 声纹识别后填充）
    pub speaker_id: Option<Uuid>,
}

impl ConversationTurn {
    pub fn new(user_text: String, response_text: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_text,
            response_text,
            timestamp: Utc::now(),
            speaker_id: None,
        }
    }
}
