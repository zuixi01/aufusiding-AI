//! 主动智能子模块
//!
//! 日报、习惯学习、通知管理、健康监控、能耗报告、日历同步、天气、建议引擎。

pub mod calendar_sync;
pub mod daily_briefing;
pub mod energy_reporter;
pub mod health_monitor;
pub mod notification_hub;
pub mod notification_rules;
pub mod routine_learner;
pub mod routine_store;
pub mod suggestion_engine;
pub mod weather_service;
pub mod media_service;
pub mod caldav_provider;
pub mod http_media_provider;

#[cfg(test)]
mod tests;

pub use daily_briefing::DailyBriefing;
pub use energy_reporter::EnergyReporter;
pub use health_monitor::HealthMonitor;
pub use notification_hub::{Notification, NotificationHub, NotificationPriority};
pub use routine_learner::RoutineLearner;
pub use suggestion_engine::SuggestionEngine;
pub use media_service::{MediaService, MediaProvider, MediaState, PlaybackStatus, TrackInfo};
