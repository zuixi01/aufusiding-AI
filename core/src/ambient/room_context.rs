//! 房间上下文感知
//!
//! 追踪用户在哪个房间/设备说话，支持多房间感知。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 房间上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomContext {
    /// 当前房间名称
    pub current_room: String,
    /// 设备 ID（本机标识）
    pub device_id: String,
    /// 最后活动时间
    pub last_activity: DateTime<Utc>,
    /// 已知房间列表
    pub known_rooms: Vec<RoomInfo>,
}

/// 房间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub name: String,
    pub device_ids: Vec<String>,
}

impl RoomContext {
    pub fn new(room: String, device_id: String) -> Self {
        Self {
            current_room: room.clone(),
            device_id,
            last_activity: Utc::now(),
            known_rooms: vec![RoomInfo {
                name: room,
                device_ids: Vec::new(),
            }],
        }
    }

    /// 更新当前房间
    pub fn update_room(&mut self, room: &str) {
        self.current_room = room.to_string();
        self.last_activity = Utc::now();
        if !self.known_rooms.iter().any(|r| r.name == room) {
            self.known_rooms.push(RoomInfo {
                name: room.to_string(),
                device_ids: Vec::new(),
            });
        }
    }

    /// 注册设备到房间
    pub fn register_device(&mut self, room: &str, device_id: &str) {
        if let Some(info) = self.known_rooms.iter_mut().find(|r| r.name == room) {
            if !info.device_ids.contains(&device_id.to_string()) {
                info.device_ids.push(device_id.to_string());
            }
        } else {
            self.known_rooms.push(RoomInfo {
                name: room.to_string(),
                device_ids: vec![device_id.to_string()],
            });
        }
    }

    /// 获取当前房间的设备列表
    pub fn current_room_devices(&self) -> Vec<&str> {
        self.known_rooms
            .iter()
            .find(|r| r.name == self.current_room)
            .map(|r| r.device_ids.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}
