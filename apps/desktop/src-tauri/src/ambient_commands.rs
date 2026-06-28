//! Ambient 模块 Tauri 命令

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;
use tauri::State;

/// Ambient 状态（前端可查询）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientStatusResponse {
    pub state: String,
    pub listening_mode: String,
    pub current_room: Option<String>,
    pub speaker: Option<String>,
    pub children_mode_enabled: bool,
    pub active_tasks: usize,
}

/// Ambient 前端状态
pub struct AmbientState {
    #[allow(dead_code)]
    pub running: Mutex<bool>,
    pub current_state: Mutex<String>,
    pub listening_mode: Mutex<String>,
    pub current_room: Mutex<Option<String>>,
    pub current_speaker: Mutex<Option<String>>,
    pub children_mode: Mutex<bool>,
    pub active_task_count: Mutex<usize>,
}

impl AmbientState {
    pub fn new() -> Self {
        Self {
            running: Mutex::new(false),
            current_state: Mutex::new("idle".into()),
            listening_mode: Mutex::new("wakeword_required".into()),
            current_room: Mutex::new(None),
            current_speaker: Mutex::new(None),
            children_mode: Mutex::new(false),
            active_task_count: Mutex::new(0),
        }
    }
}

#[tauri::command]
pub fn get_ambient_status(state: State<AmbientState>) -> Value {
    let resp = AmbientStatusResponse {
        state: state.current_state.lock().unwrap().clone(),
        listening_mode: state.listening_mode.lock().unwrap().clone(),
        current_room: state.current_room.lock().unwrap().clone(),
        speaker: state.current_speaker.lock().unwrap().clone(),
        children_mode_enabled: *state.children_mode.lock().unwrap(),
        active_tasks: *state.active_task_count.lock().unwrap(),
    };
    serde_json::to_value(resp).unwrap_or_default()
}
