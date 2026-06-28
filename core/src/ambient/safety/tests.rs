//! 安全模块测试

#[cfg(test)]
mod unit {
    use crate::ambient::biometrics::{PermissionLevel, SpeakerProfile};
    use crate::ambient::safety::children_mode::ChildrenMode;
    use crate::ambient::safety::content_filter::ContentFilter;
    use crate::ambient::safety::emergency::*;
    use crate::ambient::safety::privacy_guard::PrivacyGuard;
    use crate::ambient::safety::usage_tracker::*;
    use std::time::Duration;

    // ─── ContentFilter ─────────────────────────────────

    #[test]
    fn content_filter_blocks_keywords() {
        let filter = ContentFilter::new();
        assert!(filter.contains_blocked("这里有暴力内容"));
        assert!(!filter.contains_blocked("今天天气不错"));
    }

    #[test]
    fn content_filter_replaces_keywords() {
        let filter = ContentFilter::new();
        let result = filter.filter_text("这里有暴力内容");
        assert!(result.contains("***"));
        assert!(!result.contains("暴力"));
    }

    #[test]
    fn content_filter_add_remove_keyword() {
        let mut filter = ContentFilter::new();
        let initial = filter.keyword_count();
        filter.add_keyword("测试词");
        assert_eq!(filter.keyword_count(), initial + 1);
        assert!(filter.contains_blocked("包含测试词的文本"));

        filter.remove_keyword("测试词");
        assert_eq!(filter.keyword_count(), initial);
    }

    #[test]
    fn content_filter_no_duplicate_keywords() {
        let mut filter = ContentFilter::new();
        let initial = filter.keyword_count();
        filter.add_keyword("暴力"); // 已存在
        assert_eq!(filter.keyword_count(), initial);
    }

    // ─── ChildrenMode ──────────────────────────────────

    #[test]
    fn children_mode_blocks_child_request() {
        let mode = ChildrenMode::new(Duration::from_secs(3600));
        let child = SpeakerProfile::new("小明".into(), vec![], PermissionLevel::Child);
        assert!(mode.should_block_request("搜索暴力内容", &child));
    }

    #[test]
    fn children_mode_allows_adult_request() {
        let mode = ChildrenMode::new(Duration::from_secs(3600));
        let adult = SpeakerProfile::new("张三".into(), vec![], PermissionLevel::Admin);
        assert!(!mode.should_block_request("搜索暴力内容", &adult));
    }

    #[test]
    fn children_mode_capability_whitelist() {
        let mode = ChildrenMode::new(Duration::from_secs(3600));
        let child = SpeakerProfile::new("小明".into(), vec![], PermissionLevel::Child);
        assert!(mode.is_capability_allowed("music", &child));
        assert!(!mode.is_capability_allowed("browser", &child));
    }

    #[test]
    fn children_mode_usage_limit() {
        let mut mode = ChildrenMode::new(Duration::from_secs(3600));
        let id = uuid::Uuid::new_v4();

        assert!(mode.check_usage_limit(id));
        mode.record_usage(id, Duration::from_secs(3600));
        assert!(!mode.check_usage_limit(id));
    }

    #[test]
    fn children_mode_reset_daily() {
        let mut mode = ChildrenMode::new(Duration::from_secs(3600));
        let id = uuid::Uuid::new_v4();
        mode.record_usage(id, Duration::from_secs(3600));
        mode.reset_daily_usage();
        assert!(mode.check_usage_limit(id));
    }

    #[test]
    fn children_mode_filter_response() {
        let mode = ChildrenMode::new(Duration::from_secs(3600));
        let result = mode.filter_response("这里有暴力内容");
        assert!(result.contains("***"));
    }

    #[test]
    fn children_mode_disabled_allows_all() {
        let mut mode = ChildrenMode::new(Duration::from_secs(3600));
        mode.set_enabled(false);
        let child = SpeakerProfile::new("小明".into(), vec![], PermissionLevel::Child);
        assert!(!mode.should_block_request("暴力", &child));
    }

    // ─── EmergencyDetector ─────────────────────────────

    #[test]
    fn emergency_detect_sos() {
        let detector = EmergencyDetector::new(300);
        assert_eq!(detector.check_text("救命啊"), Some(EmergencyType::Sos));
        assert_eq!(detector.check_text("help me"), Some(EmergencyType::Sos));
    }

    #[test]
    fn emergency_detect_fire() {
        let detector = EmergencyDetector::new(300);
        assert_eq!(detector.check_text("着火了快跑"), Some(EmergencyType::Fire));
    }

    #[test]
    fn emergency_detect_medical() {
        let detector = EmergencyDetector::new(300);
        assert_eq!(detector.check_text("我胸闷"), Some(EmergencyType::Medical));
    }

    #[test]
    fn emergency_no_detection() {
        let detector = EmergencyDetector::new(300);
        assert_eq!(detector.check_text("今天天气不错"), None);
    }

    #[test]
    fn emergency_trigger_alert() {
        let mut detector = EmergencyDetector::new(0); // 无冷却期
        let alert = detector.trigger_alert("救命", None, Some("客厅"));
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.alert_type, EmergencyType::Sos);
        assert_eq!(alert.room, Some("客厅".into()));
        assert_eq!(detector.alert_count(), 1);
    }

    #[test]
    fn emergency_cooldown() {
        let mut detector = EmergencyDetector::new(300); // 5 分钟冷却
        let first = detector.trigger_alert("救命", None, None);
        assert!(first.is_some());

        // 冷却期内不应再次触发
        let second = detector.trigger_alert("救命", None, None);
        assert!(second.is_none());
    }

    #[test]
    fn emergency_add_contact() {
        let mut detector = EmergencyDetector::new(300);
        detector.add_contact(EmergencyContact {
            name: "张三".into(),
            phone: "13800138000".into(),
            notify_method: NotifyMethod::All,
        });
        assert_eq!(detector.contacts().len(), 1);
    }

    #[test]
    fn emergency_custom_keyword() {
        let mut detector = EmergencyDetector::new(300);
        detector.add_keyword("地震", EmergencyType::Sos);
        assert_eq!(detector.check_text("地震了"), Some(EmergencyType::Sos));
    }

    // ─── PrivacyGuard ──────────────────────────────────

    #[test]
    fn privacy_sanitize_phone() {
        let guard = PrivacyGuard::new();
        let result = guard.sanitize("我的电话是13800138000");
        assert!(result.contains("[电话号码]"));
        assert!(!result.contains("13800138000"));
    }

    #[test]
    fn privacy_sanitize_email() {
        let guard = PrivacyGuard::new();
        let result = guard.sanitize("邮箱是test@example.com");
        assert!(result.contains("[邮箱]"));
    }

    #[test]
    fn privacy_contains_sensitive() {
        let guard = PrivacyGuard::new();
        assert!(guard.contains_sensitive("我的号码是13800138000"));
        assert!(!guard.contains_sensitive("今天天气不错"));
    }

    #[test]
    fn privacy_no_sensitive_unchanged() {
        let guard = PrivacyGuard::new();
        let text = "今天天气不错";
        assert_eq!(guard.sanitize(text), text);
    }

    // ─── UsageTracker ──────────────────────────────────

    #[test]
    fn usage_tracker_record_and_count() {
        let mut tracker = UsageTracker::new(1000);
        tracker.record(UsageRecord {
            speaker_id: None,
            feature: "iot".into(),
            timestamp: chrono::Utc::now(),
            duration_ms: 500,
        });
        tracker.record(UsageRecord {
            speaker_id: None,
            feature: "chat".into(),
            timestamp: chrono::Utc::now(),
            duration_ms: 1000,
        });
        tracker.record(UsageRecord {
            speaker_id: None,
            feature: "iot".into(),
            timestamp: chrono::Utc::now(),
            duration_ms: 300,
        });

        assert_eq!(tracker.record_count(), 3);
        assert_eq!(tracker.total_duration_ms(), 1800);

        let by_feature = tracker.count_by_feature();
        assert_eq!(by_feature["iot"], 2);
        assert_eq!(by_feature["chat"], 1);
    }

    #[test]
    fn usage_tracker_most_used() {
        let mut tracker = UsageTracker::new(1000);
        for _ in 0..5 {
            tracker.record(UsageRecord {
                speaker_id: None,
                feature: "iot".into(),
                timestamp: chrono::Utc::now(),
                duration_ms: 100,
            });
        }
        for _ in 0..3 {
            tracker.record(UsageRecord {
                speaker_id: None,
                feature: "chat".into(),
                timestamp: chrono::Utc::now(),
                duration_ms: 100,
            });
        }
        assert_eq!(tracker.most_used_feature(), Some("iot".into()));
    }

    #[test]
    fn usage_tracker_max_records() {
        let mut tracker = UsageTracker::new(3);
        for i in 0..5 {
            tracker.record(UsageRecord {
                speaker_id: None,
                feature: format!("f{}", i),
                timestamp: chrono::Utc::now(),
                duration_ms: 100,
            });
        }
        assert_eq!(tracker.record_count(), 3);
    }

    #[test]
    fn usage_tracker_clear() {
        let mut tracker = UsageTracker::new(100);
        tracker.record(UsageRecord {
            speaker_id: None,
            feature: "test".into(),
            timestamp: chrono::Utc::now(),
            duration_ms: 100,
        });
        tracker.clear();
        assert_eq!(tracker.record_count(), 0);
    }
}
