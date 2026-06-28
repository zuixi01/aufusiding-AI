//! 说话人档案
//!
//! 家庭成员档案：姓名、声纹、偏好、权限等级。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 权限等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    /// 管理员（完全控制）
    Admin,
    /// 标准用户
    Standard,
    /// 儿童（受限）
    Child,
    /// 访客（最低权限）
    Guest,
}

/// 说话人档案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerProfile {
    pub id: Uuid,
    pub name: String,
    /// 声纹 embedding（序列化为 Vec<f32>）
    pub voiceprint: Vec<f32>,
    pub is_child: bool,
    pub permission_level: PermissionLevel,
    /// 个人偏好（如 "tts_speed" -> "fast"）
    pub preferences: HashMap<String, String>,
}

impl SpeakerProfile {
    pub fn new(name: String, voiceprint: Vec<f32>, permission_level: PermissionLevel) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            voiceprint,
            is_child: permission_level == PermissionLevel::Child,
            permission_level,
            preferences: HashMap::new(),
        }
    }

    /// 创建访客档案（无声纹）
    pub fn guest() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "访客".into(),
            voiceprint: Vec::new(),
            is_child: false,
            permission_level: PermissionLevel::Guest,
            preferences: HashMap::new(),
        }
    }

    /// 设置偏好
    pub fn set_preference(&mut self, key: &str, value: &str) {
        self.preferences.insert(key.to_string(), value.to_string());
    }

    /// 获取偏好
    pub fn get_preference(&self, key: &str) -> Option<&str> {
        self.preferences.get(key).map(|s| s.as_str())
    }

    /// 是否有足够权限执行操作
    pub fn has_permission(&self, required: PermissionLevel) -> bool {
        matches!((self.permission_level, required),
            (PermissionLevel::Admin, _)
            | (PermissionLevel::Standard, PermissionLevel::Standard)
            | (PermissionLevel::Standard, PermissionLevel::Child)
            | (PermissionLevel::Standard, PermissionLevel::Guest)
            | (PermissionLevel::Child, PermissionLevel::Child)
            | (PermissionLevel::Child, PermissionLevel::Guest)
            | (PermissionLevel::Guest, PermissionLevel::Guest)
        )
    }
}
