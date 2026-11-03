//! 主动智能模块测试

#[cfg(test)]
mod unit {
    use crate::ambient::proactive::calendar_sync::*;
    use crate::ambient::proactive::daily_briefing::*;
    use crate::ambient::proactive::energy_reporter::*;
    use crate::ambient::proactive::health_monitor::*;
    use crate::ambient::proactive::notification_hub::*;
    use crate::ambient::proactive::notification_rules::*;
    use crate::ambient::proactive::routine_learner::*;
    use crate::ambient::proactive::routine_store::*;
    use crate::ambient::proactive::suggestion_engine::*;
    use crate::ambient::proactive::weather_service::*;
    use std::sync::Arc;

    // ─── NotificationHub ───────────────────────────────

    #[test]
    fn notification_push_and_count() {
        let hub = NotificationHub::new();
        hub.push(Notification::new(
            "微信",
            "新消息",
            "你好",
            NotificationPriority::Normal,
        ));
        hub.push(Notification::new(
            "邮件",
            "会议",
            "明天开会",
            NotificationPriority::High,
        ));
        assert_eq!(hub.pending_count(), 2);
        assert_eq!(hub.total_count(), 2);
    }

    #[test]
    fn notification_priority_sorting() {
        let hub = NotificationHub::new();
        hub.push(Notification::new("a", "低", "x", NotificationPriority::Low));
        hub.push(Notification::new(
            "b",
            "紧急",
            "y",
            NotificationPriority::Critical,
        ));
        hub.push(Notification::new(
            "c",
            "普通",
            "z",
            NotificationPriority::Normal,
        ));

        let sorted = hub.pending_sorted();
        assert_eq!(sorted[0].priority, NotificationPriority::Critical);
        assert_eq!(sorted[2].priority, NotificationPriority::Low);
    }

    #[test]
    fn notification_dismiss() {
        let hub = NotificationHub::new();
        let n = Notification::new("a", "test", "x", NotificationPriority::Normal);
        let id = n.id;
        hub.push(n);
        assert_eq!(hub.pending_count(), 1);

        hub.dismiss(&[id]);
        assert_eq!(hub.pending_count(), 0);
    }

    #[test]
    fn notification_dismiss_all() {
        let hub = NotificationHub::new();
        hub.push(Notification::new(
            "a",
            "1",
            "x",
            NotificationPriority::Normal,
        ));
        hub.push(Notification::new("b", "2", "y", NotificationPriority::High));
        hub.dismiss_all();
        assert_eq!(hub.pending_count(), 0);
    }

    #[test]
    fn notification_voice_summary_empty() {
        let hub = NotificationHub::new();
        assert_eq!(hub.voice_summary(), "没有新通知。");
    }

    #[test]
    fn notification_voice_summary_with_items() {
        let hub = NotificationHub::new();
        hub.push(Notification::new(
            "微信",
            "张三",
            "你好",
            NotificationPriority::Normal,
        ));
        hub.push(Notification::new(
            "邮件",
            "会议通知",
            "明天开会",
            NotificationPriority::Critical,
        ));
        let summary = hub.voice_summary();
        assert!(summary.contains("2 条新通知"));
        assert!(summary.contains("1 条紧急"));
    }

    #[test]
    fn notification_clear_read() {
        let hub = NotificationHub::new();
        hub.push(Notification::new(
            "a",
            "1",
            "x",
            NotificationPriority::Normal,
        ));
        hub.dismiss_all();
        hub.clear_read();
        assert_eq!(hub.total_count(), 0);
    }

    // ─── NotificationRules ─────────────────────────────

    #[test]
    fn rule_engine_block_source() {
        let mut engine = NotificationRuleEngine::new();
        engine.add_rule(NotificationRule {
            name: "block_spam".into(),
            source_pattern: Some("广告".into()),
            min_priority: None,
            action: FilterAction::Block,
            enabled: true,
        });

        let n = Notification::new("广告推送", "促销", "打折", NotificationPriority::Low);
        assert_eq!(engine.evaluate(&n), FilterAction::Block);

        let n2 = Notification::new("微信", "消息", "你好", NotificationPriority::Normal);
        assert_eq!(engine.evaluate(&n2), FilterAction::Allow);
    }

    #[test]
    fn rule_engine_disabled_rule_ignored() {
        let mut engine = NotificationRuleEngine::new();
        engine.add_rule(NotificationRule {
            name: "disabled".into(),
            source_pattern: Some("微信".into()),
            min_priority: None,
            action: FilterAction::Block,
            enabled: false,
        });

        let n = Notification::new("微信", "消息", "你好", NotificationPriority::Normal);
        assert_eq!(engine.evaluate(&n), FilterAction::Allow);
    }

    // ─── HealthMonitor ─────────────────────────────────

    #[test]
    fn health_monitor_basic() {
        let mut monitor = HealthMonitor::new();
        monitor.update_heart_rate(72);
        monitor.update_steps(5000);
        assert_eq!(monitor.heart_rate(), Some(72));
        assert_eq!(monitor.steps_today(), 5000);
    }

    #[test]
    fn health_monitor_anomaly_high_hr() {
        let mut monitor = HealthMonitor::new();
        monitor.update_heart_rate(150);
        let alerts = monitor.check_anomalies();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, HealthAlertType::HeartRateHigh);
    }

    #[test]
    fn health_monitor_anomaly_low_hr() {
        let mut monitor = HealthMonitor::new();
        monitor.update_heart_rate(40);
        let alerts = monitor.check_anomalies();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, HealthAlertType::HeartRateLow);
    }

    #[test]
    fn health_monitor_sleep_deficit() {
        let mut monitor = HealthMonitor::new();
        monitor.update_sleep(SleepSummary {
            total_hours: 4.5,
            deep_sleep_hours: 1.0,
            light_sleep_hours: 2.5,
            rem_hours: 1.0,
            awake_minutes: 30,
        });
        let alerts = monitor.check_anomalies();
        assert!(alerts
            .iter()
            .any(|a| a.alert_type == HealthAlertType::SleepDeficit));
    }

    #[test]
    fn health_monitor_no_anomaly() {
        let mut monitor = HealthMonitor::new();
        monitor.update_heart_rate(72);
        monitor.update_sleep(SleepSummary {
            total_hours: 7.5,
            deep_sleep_hours: 2.0,
            light_sleep_hours: 4.0,
            rem_hours: 1.5,
            awake_minutes: 15,
        });
        assert!(monitor.check_anomalies().is_empty());
    }

    #[test]
    fn health_daily_report() {
        let mut monitor = HealthMonitor::new();
        monitor.update_heart_rate(72);
        monitor.update_steps(6000);
        monitor.update_sleep(SleepSummary {
            total_hours: 7.5,
            deep_sleep_hours: 2.0,
            light_sleep_hours: 4.0,
            rem_hours: 1.5,
            awake_minutes: 15,
        });
        let report = monitor.daily_report();
        assert!(report.contains("7.5 小时"));
        assert!(report.contains("6000"));
        assert!(report.contains("72 bpm"));
    }

    // ─── EnergyReporter ────────────────────────────────

    #[test]
    fn energy_reporter_basic() {
        let mut reporter = EnergyReporter::new();
        reporter.update_device(DeviceConsumption {
            device_id: "ac-1".into(),
            device_name: "空调".into(),
            watts: 1500.0,
        });
        reporter.update_device(DeviceConsumption {
            device_id: "light-1".into(),
            device_name: "客厅灯".into(),
            watts: 60.0,
        });

        assert_eq!(reporter.device_count(), 2);
        assert_eq!(reporter.current_consumption(), 1560.0);
    }

    #[test]
    fn energy_daily_report() {
        let mut reporter = EnergyReporter::new();
        reporter.update_device(DeviceConsumption {
            device_id: "ac-1".into(),
            device_name: "空调".into(),
            watts: 1500.0,
        });
        let report = reporter.daily_report();
        assert!(report.contains("空调"));
        assert!(report.contains("1500"));
    }

    #[test]
    fn energy_suggest_savings() {
        let mut reporter = EnergyReporter::new();
        reporter.update_device(DeviceConsumption {
            device_id: "heater".into(),
            device_name: "电暖器".into(),
            watts: 2000.0,
        });
        let suggestions = reporter.suggest_savings();
        assert!(suggestions.iter().any(|s| s.contains("电暖器")));
    }

    #[test]
    fn energy_empty_report() {
        let reporter = EnergyReporter::new();
        assert_eq!(reporter.daily_report(), "暂无能耗数据。");
    }

    // ─── CalendarSync ──────────────────────────────────

    #[test]
    fn calendar_add_and_query() {
        let mut cal = CalendarSync::new();
        let now = chrono::Utc::now();
        cal.add_event(CalendarEvent {
            id: uuid::Uuid::new_v4(),
            title: "产品评审".into(),
            start: now + chrono::Duration::hours(1),
            end: now + chrono::Duration::hours(2),
            location: Some("会议室A".into()),
            description: None,
            is_all_day: false,
            attendees: Vec::new(),
            reminders: Vec::new(),
        });

        let today = cal.today_events(now);
        assert_eq!(today.len(), 1);
        assert_eq!(today[0].title, "产品评审");
    }

    #[test]
    fn calendar_next_event() {
        let mut cal = CalendarSync::new();
        let now = chrono::Utc::now();
        cal.add_event(CalendarEvent {
            id: uuid::Uuid::new_v4(),
            title: "午餐".into(),
            start: now + chrono::Duration::hours(3),
            end: now + chrono::Duration::hours(4),
            location: None,
            description: None,
            is_all_day: false,
            attendees: Vec::new(),
            reminders: Vec::new(),
        });
        cal.add_event(CalendarEvent {
            id: uuid::Uuid::new_v4(),
            title: "会议".into(),
            start: now + chrono::Duration::hours(1),
            end: now + chrono::Duration::hours(2),
            location: None,
            description: None,
            is_all_day: false,
            attendees: Vec::new(),
            reminders: Vec::new(),
        });

        let next = cal.next_event(now).unwrap();
        assert_eq!(next.title, "会议");
    }

    #[test]
    fn calendar_voice_summary_empty() {
        let cal = CalendarSync::new();
        let summary = cal.voice_summary(chrono::Utc::now());
        assert!(summary.contains("没有日程"));
    }

    // ─── WeatherService ────────────────────────────────

    #[tokio::test]
    async fn weather_current() {
        let service = WeatherService::new(Box::new(MockWeatherProvider));
        let info = service.current("北京").await.unwrap();
        assert_eq!(info.condition, "多云");
        assert_eq!(info.temperature_c, 22.0);
    }

    #[tokio::test]
    async fn weather_forecast() {
        let service = WeatherService::new(Box::new(MockWeatherProvider));
        let forecast = service.forecast("北京", 3).await.unwrap();
        assert_eq!(forecast.len(), 3);
    }

    #[tokio::test]
    async fn weather_voice_summary() {
        let service = WeatherService::new(Box::new(MockWeatherProvider));
        let summary = service.voice_summary("北京").await.unwrap();
        assert!(summary.contains("多云"));
        assert!(summary.contains("22"));
    }

    // ─── RoutineStore ──────────────────────────────────

    #[test]
    fn routine_store_record_and_query() {
        let mut store = RoutineStore::new(100);
        store.record(UserEvent::new("turn_off_light", "关客厅灯"));
        store.record(UserEvent::new("turn_off_light", "关客厅灯"));
        store.record(UserEvent::new("lock_door", "锁门"));

        assert_eq!(store.count(), 3);
        assert_eq!(store.events_by_type("turn_off_light").len(), 2);
        assert_eq!(store.events_by_type("lock_door").len(), 1);
    }

    #[test]
    fn routine_store_max_events() {
        let mut store = RoutineStore::new(3);
        for i in 0..5 {
            store.record(UserEvent::new(&format!("event_{}", i), "test"));
        }
        assert_eq!(store.count(), 3);
    }

    #[test]
    fn routine_store_clear() {
        let mut store = RoutineStore::new(100);
        store.record(UserEvent::new("test", "test"));
        store.clear();
        assert_eq!(store.count(), 0);
    }

    // ─── RoutineLearner ────────────────────────────────

    #[test]
    fn routine_learner_no_patterns_below_threshold() {
        let mut learner = RoutineLearner::new(5, 0.5);
        for _ in 0..3 {
            learner.record_event(UserEvent::new("turn_off_light", "关灯"));
        }
        let patterns = learner.analyze_patterns();
        assert!(patterns.is_empty()); // 不足 5 次
    }

    #[test]
    fn routine_learner_event_count() {
        let mut learner = RoutineLearner::new(5, 0.5);
        learner.record_event(UserEvent::new("a", "test"));
        learner.record_event(UserEvent::new("b", "test"));
        assert_eq!(learner.event_count(), 2);
    }

    // ─── SuggestionEngine ──────────────────────────────

    #[test]
    fn suggestion_engine_empty() {
        let learner = RoutineLearner::new(5, 0.5);
        let engine = SuggestionEngine::new(5);
        let suggestions = engine.from_routines(&learner);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggestion_voice_text_empty() {
        let engine = SuggestionEngine::new(5);
        assert!(engine.voice_text(&[]).is_empty());
    }

    // ─── DailyBriefing ────────────────────────────────

    #[tokio::test]
    async fn daily_briefing_generate() {
        let weather = WeatherService::new(Box::new(MockWeatherProvider));
        let calendar = CalendarSync::new();
        let hub = Arc::new(NotificationHub::new());
        let health = Arc::new(HealthMonitor::new());

        hub.push(Notification::new(
            "微信",
            "消息",
            "你好",
            NotificationPriority::Normal,
        ));

        let briefing = DailyBriefing::new(weather, calendar, hub, health, "北京".into());
        let text = briefing.generate("张三").await.unwrap();

        assert!(text.contains("早上好，张三"));
        assert!(text.contains("多云") || text.contains("天气"));
        assert!(text.contains("1 条未读"));
    }
}
