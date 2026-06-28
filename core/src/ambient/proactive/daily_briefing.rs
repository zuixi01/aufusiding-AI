//! 每日简报
//!
//! 天气/日历/通勤/新闻/待办 综合简报。

use std::sync::Arc;

use super::calendar_sync::CalendarSync;
use super::health_monitor::HealthMonitor;
use super::notification_hub::NotificationHub;
use super::weather_service::WeatherService;

/// 每日简报生成器
pub struct DailyBriefing {
    weather: WeatherService,
    calendar: CalendarSync,
    notification_hub: Arc<NotificationHub>,
    health: Arc<HealthMonitor>,
    /// 默认城市
    city: String,
}

impl DailyBriefing {
    pub fn new(
        weather: WeatherService,
        calendar: CalendarSync,
        notification_hub: Arc<NotificationHub>,
        health: Arc<HealthMonitor>,
        city: String,
    ) -> Self {
        Self {
            weather,
            calendar,
            notification_hub,
            health,
            city,
        }
    }

    /// 生成每日简报文本（TTS 友好格式）
    pub async fn generate(&self, greeting_name: &str) -> anyhow::Result<String> {
        let mut parts = Vec::new();

        // 问候
        parts.push(format!("早上好，{}。", greeting_name));

        // 天气
        match self.weather.voice_summary(&self.city).await {
            Ok(weather) => parts.push(format!("今天{}", weather)),
            Err(_) => parts.push("天气信息暂时无法获取。".into()),
        }

        // 日历
        let now = chrono::Utc::now();
        let calendar_summary = self.calendar.voice_summary(now);
        parts.push(calendar_summary);

        // 健康
        let health_report = self.health.daily_report();
        if !health_report.is_empty() {
            parts.push(health_report);
        }

        // 通知
        let pending = self.notification_hub.pending_count();
        if pending > 0 {
            parts.push(format!("你有 {} 条未读通知。", pending));
        }

        Ok(parts.join(" "))
    }

    /// 获取城市
    pub fn city(&self) -> &str {
        &self.city
    }
}
