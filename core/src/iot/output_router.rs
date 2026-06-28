//! 输出路由器
//!
//! 根据内容类型 + 在线设备智能选择输出目标。

use serde::{Deserialize, Serialize};

use super::content_analyzer::{ContentAnalyzer, ContentType};
use super::types::DeviceType;

/// 输出候选设备
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputCandidate {
    pub device_id: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub score: f32,
    pub reason: String,
}

/// 输出路由器
pub struct OutputRouter {
    content_analyzer: ContentAnalyzer,
    /// 已知在线设备（简化：实际会查询 DeviceRegistry）
    known_devices: Vec<OutputDeviceInfo>,
}

/// 输出设备信息
#[derive(Debug, Clone)]
pub struct OutputDeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub room: String,
    pub has_screen: bool,
    pub has_speaker: bool,
    pub screen_size_inch: Option<f32>,
    pub online: bool,
}

impl OutputRouter {
    pub fn new() -> Self {
        Self {
            content_analyzer: ContentAnalyzer::new(),
            known_devices: Vec::new(),
        }
    }

    /// 注册输出设备
    pub fn register_device(&mut self, device: OutputDeviceInfo) {
        self.known_devices.push(device);
    }

    /// 根据内容类型和用户房间选择最佳输出设备
    pub fn select_best_output(
        &self,
        content_type: ContentType,
        user_room: &str,
    ) -> Vec<OutputCandidate> {
        let needs_screen = self.content_analyzer.needs_screen(content_type);
        let voice_friendly = self.content_analyzer.is_voice_friendly(content_type);

        let mut candidates: Vec<OutputCandidate> = self
            .known_devices
            .iter()
            .filter(|d| d.online)
            .map(|d| {
                let mut score: f32 = 0.0;
                let mut reasons = Vec::new();

                // 同房间加分
                if d.room == user_room {
                    score += 0.3;
                    reasons.push("同房间");
                }

                // 需要屏幕时，有屏幕的设备加分
                if needs_screen && d.has_screen {
                    score += 0.4;
                    reasons.push("有屏幕");
                    // 大屏额外加分
                    if let Some(size) = d.screen_size_inch {
                        if size >= 40.0 {
                            score += 0.2;
                            reasons.push("大屏适合展示");
                        }
                    }
                }

                // 语音友好内容，有音箱的设备加分
                if voice_friendly && d.has_speaker {
                    score += 0.3;
                    reasons.push("有音箱");
                }

                // 设备类型匹配
                match (&d.device_type, content_type) {
                    (DeviceType::SmartTv, ContentType::Video) => {
                        score += 0.3;
                        reasons.push("电视适合播放视频");
                    }
                    (DeviceType::Speaker, ContentType::Audio) => {
                        score += 0.3;
                        reasons.push("音箱适合播放音频");
                    }
                    _ => {}
                }

                OutputCandidate {
                    device_id: d.device_id.clone(),
                    device_name: d.device_name.clone(),
                    device_type: d.device_type.clone(),
                    score,
                    reason: reasons.join("，"),
                }
            })
            .collect();

        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates
    }

    /// 无可用输出设备时降级为语音
    pub fn fallback_to_voice(&self, content_type: ContentType) -> String {
        match content_type {
            ContentType::Report => "报告内容较长，我为您语音摘要。".into(),
            ContentType::Chart => "图表无法语音展示，我为您描述数据趋势。".into(),
            ContentType::Image => "图片无法语音展示，已保存到本地。".into(),
            ContentType::Video => "视频无法语音播放，已保存到本地。".into(),
            _ => "".into(),
        }
    }
}
