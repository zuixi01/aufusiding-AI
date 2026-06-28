//! 紧急检测
//!
//! 紧急关键词检测 + SOS 警报 + 紧急联系人通知。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::ambient::biometrics::SpeakerProfile;

/// 紧急类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmergencyType {
    Fall,
    Fire,
    Medical,
    Sos,
    Intrusion,
}

/// 通知方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyMethod {
    Sms,
    Call,
    Push,
    All,
}

/// 紧急联系人
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyContact {
    pub name: String,
    pub phone: String,
    pub notify_method: NotifyMethod,
}

/// 紧急警报
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyAlert {
    pub alert_type: EmergencyType,
    pub transcript: String,
    pub speaker: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub room: Option<String>,
}

/// 紧急检测器
pub struct EmergencyDetector {
    /// 紧急关键词 → 类型映射
    keywords: Vec<(String, EmergencyType)>,
    /// 紧急联系人
    contacts: Vec<EmergencyContact>,
    /// 冷却期（防止重复触发）
    cooldown_secs: u64,
    /// 上次触发时间
    last_triggered: Option<Instant>,
    /// 已触发的警报记录
    alert_history: Vec<EmergencyAlert>,
}

impl EmergencyDetector {
    pub fn new(cooldown_secs: u64) -> Self {
        Self {
            keywords: vec![
                ("救命".into(), EmergencyType::Sos),
                ("help".into(), EmergencyType::Sos),
                ("sos".into(), EmergencyType::Sos),
                ("摔倒了".into(), EmergencyType::Fall),
                ("着火了".into(), EmergencyType::Fire),
                ("火灾".into(), EmergencyType::Fire),
                ("心脏".into(), EmergencyType::Medical),
                ("胸闷".into(), EmergencyType::Medical),
                ("呼吸困难".into(), EmergencyType::Medical),
                ("有人闯入".into(), EmergencyType::Intrusion),
                ("小偷".into(), EmergencyType::Intrusion),
            ],
            contacts: Vec::new(),
            cooldown_secs,
            last_triggered: None,
            alert_history: Vec::new(),
        }
    }

    /// 添加紧急联系人
    pub fn add_contact(&mut self, contact: EmergencyContact) {
        self.contacts.push(contact);
    }

    /// 添加自定义关键词
    pub fn add_keyword(&mut self, keyword: &str, emergency_type: EmergencyType) {
        self.keywords.push((keyword.to_string(), emergency_type));
    }

    /// 检测文本中的紧急关键词
    pub fn check_text(&self, text: &str) -> Option<EmergencyType> {
        let lower = text.to_lowercase();
        for (kw, etype) in &self.keywords {
            if lower.contains(kw) {
                return Some(*etype);
            }
        }
        None
    }

    /// 触发警报（检查冷却期）
    pub fn trigger_alert(
        &mut self,
        text: &str,
        speaker: Option<&SpeakerProfile>,
        room: Option<&str>,
    ) -> Option<EmergencyAlert> {
        // 检查冷却期
        if let Some(last) = self.last_triggered {
            if last.elapsed().as_secs() < self.cooldown_secs {
                return None;
            }
        }

        let emergency_type = self.check_text(text)?;

        let alert = EmergencyAlert {
            alert_type: emergency_type,
            transcript: text.to_string(),
            speaker: speaker.map(|s| s.name.clone()),
            timestamp: Utc::now(),
            room: room.map(|r| r.to_string()),
        };

        self.last_triggered = Some(Instant::now());
        self.alert_history.push(alert.clone());

        tracing::warn!(
            alert_type = ?emergency_type,
            transcript = %text,
            "紧急警报触发"
        );

        Some(alert)
    }

    /// 获取紧急联系人列表
    pub fn contacts(&self) -> &[EmergencyContact] {
        &self.contacts
    }

    /// 警报历史数
    pub fn alert_count(&self) -> usize {
        self.alert_history.len()
    }

    /// 关键词数
    pub fn keyword_count(&self) -> usize {
        self.keywords.len()
    }
}
