//! IoT 输出路由 + 新适配器测试

#[cfg(test)]
mod unit {
    use crate::iot::adapters::hdmi_cec::*;
    use crate::iot::adapters::ir_bridge::*;
    use crate::iot::adapters::projector::*;
    use crate::iot::casting::*;
    use crate::iot::content_analyzer::*;
    use crate::iot::control::DeviceController;
    use crate::iot::device_handoff::*;
    use crate::iot::geofence_trigger::*;
    use crate::iot::output_router::*;
    use crate::iot::types::DeviceType;

    // ─── ContentAnalyzer ───────────────────────────────

    #[test]
    fn analyze_short_text_is_short_message() {
        let a = ContentAnalyzer::new();
        assert_eq!(a.analyze("你好", None), ContentType::ShortMessage);
    }

    #[test]
    fn analyze_long_text_is_report() {
        let a = ContentAnalyzer::new();
        let long = "这是一段很长的报告内容。".repeat(50);
        assert_eq!(a.analyze(&long, None), ContentType::Report);
    }

    #[test]
    fn analyze_video_mime() {
        let a = ContentAnalyzer::new();
        assert_eq!(a.analyze("", Some("video/mp4")), ContentType::Video);
    }

    #[test]
    fn analyze_image_mime() {
        let a = ContentAnalyzer::new();
        assert_eq!(a.analyze("", Some("image/png")), ContentType::Image);
    }

    #[test]
    fn analyze_chart_keywords() {
        let a = ContentAnalyzer::new();
        // 需要超过 100 个 chars 才不会被判为 ShortMessage
        let text = "以下是销售数据统计图表，展示了过去一年的趋势变化，包含多个维度的分析结果和对比数据。这份报告涵盖了各季度的详细统计信息和图表展示，同时还包括了各地区的销售对比分析，以及未来三个季度的预测数据和增长趋势分析报告。";
        assert!(
            text.chars().count() > 100,
            "test text should be >100 chars, got {}",
            text.chars().count()
        );
        assert_eq!(a.analyze(text, None), ContentType::Chart);
    }

    #[test]
    fn voice_friendly_types() {
        let a = ContentAnalyzer::new();
        assert!(a.is_voice_friendly(ContentType::ShortMessage));
        assert!(a.is_voice_friendly(ContentType::Text));
        assert!(!a.is_voice_friendly(ContentType::Video));
    }

    #[test]
    fn needs_screen_types() {
        let a = ContentAnalyzer::new();
        assert!(a.needs_screen(ContentType::Report));
        assert!(a.needs_screen(ContentType::Video));
        assert!(!a.needs_screen(ContentType::ShortMessage));
    }

    // ─── OutputRouter ──────────────────────────────────

    #[test]
    fn output_router_selects_tv_for_report() {
        let mut router = OutputRouter::new();
        router.register_device(OutputDeviceInfo {
            device_id: "tv-1".into(),
            device_name: "客厅电视".into(),
            device_type: DeviceType::SmartTv,
            room: "客厅".into(),
            has_screen: true,
            has_speaker: true,
            screen_size_inch: Some(55.0),
            online: true,
        });
        router.register_device(OutputDeviceInfo {
            device_id: "speaker-1".into(),
            device_name: "卧室音箱".into(),
            device_type: DeviceType::Speaker,
            room: "卧室".into(),
            has_screen: false,
            has_speaker: true,
            screen_size_inch: None,
            online: true,
        });

        let candidates = router.select_best_output(ContentType::Report, "客厅");
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].device_id, "tv-1");
    }

    #[test]
    fn output_router_selects_speaker_for_audio() {
        let mut router = OutputRouter::new();
        router.register_device(OutputDeviceInfo {
            device_id: "speaker-1".into(),
            device_name: "客厅音箱".into(),
            device_type: DeviceType::Speaker,
            room: "客厅".into(),
            has_screen: false,
            has_speaker: true,
            screen_size_inch: None,
            online: true,
        });

        let candidates = router.select_best_output(ContentType::Audio, "客厅");
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].device_id, "speaker-1");
    }

    #[test]
    fn output_router_filters_offline_devices() {
        let mut router = OutputRouter::new();
        router.register_device(OutputDeviceInfo {
            device_id: "tv-1".into(),
            device_name: "电视".into(),
            device_type: DeviceType::SmartTv,
            room: "客厅".into(),
            has_screen: true,
            has_speaker: true,
            screen_size_inch: Some(55.0),
            online: false,
        });

        let candidates = router.select_best_output(ContentType::Report, "客厅");
        assert!(candidates.is_empty());
    }

    #[test]
    fn output_router_fallback_to_voice() {
        let router = OutputRouter::new();
        let msg = router.fallback_to_voice(ContentType::Report);
        assert!(!msg.is_empty());
    }

    // ─── IrBridgeAdapter ───────────────────────────────

    #[tokio::test]
    async fn ir_bridge_store_and_send() {
        let mut adapter = IrBridgeAdapter::new("192.168.1.100".into(), IrGatewayType::BroadlinkRm4);
        adapter.store_code("ac-1", "power_on", vec![0x01, 0x02, 0x03]);
        adapter.store_code("ac-1", "power_off", vec![0x04, 0x05, 0x06]);

        assert!(adapter.get_code("ac-1", "power_on").is_some());
        assert_eq!(adapter.list_commands("ac-1").len(), 2);

        adapter.turn_on("ac-1").await.unwrap();
        let state = adapter.get_state("ac-1").await.unwrap();
        assert_eq!(state["powered"], true);
    }

    #[tokio::test]
    async fn ir_bridge_missing_code_fails() {
        let adapter = IrBridgeAdapter::new("192.168.1.100".into(), IrGatewayType::TuyaIr);
        assert!(adapter.turn_on("unknown").await.is_err());
    }

    #[test]
    fn ir_gateway_type_serde() {
        let t = IrGatewayType::BroadlinkRm4;
        let json = serde_json::to_string(&t).unwrap();
        let back: IrGatewayType = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    // ─── HdmiCecAdapter ────────────────────────────────

    #[tokio::test]
    async fn hdmi_cec_turn_on_off() {
        let adapter = HdmiCecAdapter::new("/dev/cec0".into());
        adapter.turn_on("tv-1").await.unwrap();
        let state = adapter.get_state("tv-1").await.unwrap();
        assert_eq!(state["powered"], true);

        adapter.turn_off("tv-1").await.unwrap();
        let state = adapter.get_state("tv-1").await.unwrap();
        assert_eq!(state["powered"], false);
    }

    #[tokio::test]
    async fn hdmi_cec_set_property() {
        let adapter = HdmiCecAdapter::new("/dev/cec0".into());
        adapter.turn_on("tv-1").await.unwrap();
        adapter
            .set_property("tv-1", "active_source", serde_json::json!("hdmi2"))
            .await
            .unwrap();
        let state = adapter.get_state("tv-1").await.unwrap();
        assert_eq!(state["active_source"], "hdmi2");
    }

    // ─── ProjectorAdapter ──────────────────────────────

    #[tokio::test]
    async fn projector_control() {
        let adapter = ProjectorAdapter::new("proj-1".into(), Some("192.168.1.50".into()), true);
        assert_eq!(adapter.supported_type(), "projector");

        adapter.turn_on("proj-1").await.unwrap();
        let state = adapter.get_state("proj-1").await.unwrap();
        assert_eq!(state["powered"], true);

        adapter
            .set_property("proj-1", "input_source", serde_json::json!("hdmi2"))
            .await
            .unwrap();
        let state = adapter.get_state("proj-1").await.unwrap();
        assert_eq!(state["input_source"], "hdmi2");
    }

    // ─── CastingService ────────────────────────────────

    #[tokio::test]
    async fn casting_lifecycle() {
        let service = CastingService::new();
        let target = CastTarget {
            device_id: "tv-1".into(),
            device_name: "客厅电视".into(),
            protocol: CastProtocol::Dlna,
            ip: "192.168.1.10".into(),
            port: 8080,
        };

        let session_id = service
            .cast(&target, "http://example.com/video.mp4")
            .await
            .unwrap();
        assert_eq!(service.active_count(), 1);
        assert_eq!(
            service.session_state(session_id),
            Some(CastSessionState::Casting)
        );

        service.pause(session_id).unwrap();
        assert_eq!(
            service.session_state(session_id),
            Some(CastSessionState::Paused)
        );

        service.stop(session_id).unwrap();
        assert_eq!(service.active_count(), 0);
    }

    #[tokio::test]
    async fn casting_stop_nonexistent_fails() {
        let service = CastingService::new();
        assert!(service.stop(uuid::Uuid::new_v4()).is_err());
    }

    // ─── DeviceHandoff ─────────────────────────────────

    #[tokio::test]
    async fn handoff_transfer_and_accept() {
        let handoff = DeviceHandoff::new();
        let ctx = TaskContext {
            task_id: uuid::Uuid::new_v4(),
            description: "播放音乐".into(),
            state: serde_json::json!({"track": "song.mp3"}),
            source_device: "phone-1".into(),
            created_at: chrono::Utc::now(),
        };

        let id = handoff.transfer(ctx, "speaker-1").await.unwrap();
        assert_eq!(handoff.record_count(), 1);

        let pending = handoff.pending_for_device("speaker-1");
        assert_eq!(pending.len(), 1);

        handoff.accept(id).unwrap();
        let pending = handoff.pending_for_device("speaker-1");
        assert_eq!(pending.len(), 0);
    }

    // ─── GeofenceTrigger ───────────────────────────────

    #[test]
    fn geofence_enter_exit() {
        let mut trigger = GeofenceTrigger::new();
        trigger.add_zone(GeofenceZone {
            name: "家".into(),
            lat: 39.9042,
            lng: 116.4074,
            radius_meters: 200.0,
        });
        trigger.add_rule(GeofenceRule {
            zone_name: "家".into(),
            event: GeofenceEvent::Enter,
            action_description: "打开客厅灯".into(),
            enabled: true,
        });

        // 进入围栏
        let events = trigger.update_location(39.9042, 116.4074);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, GeofenceEvent::Enter);
        assert_eq!(events[0].1, "家");

        // 匹配规则
        let rules = trigger.matching_rules(GeofenceEvent::Enter, "家");
        assert_eq!(rules.len(), 1);

        // 离开围栏
        let events = trigger.update_location(40.0, 117.0);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, GeofenceEvent::Exit);
    }

    #[test]
    fn geofence_no_event_when_staying() {
        let mut trigger = GeofenceTrigger::new();
        trigger.add_zone(GeofenceZone {
            name: "家".into(),
            lat: 39.9042,
            lng: 116.4074,
            radius_meters: 200.0,
        });

        trigger.update_location(39.9042, 116.4074);
        // 仍在围栏内，不应触发新事件
        let events = trigger.update_location(39.9043, 116.4075);
        assert!(events.is_empty());
    }

    #[test]
    fn geofence_disabled_rule_not_matched() {
        let mut trigger = GeofenceTrigger::new();
        trigger.add_zone(GeofenceZone {
            name: "家".into(),
            lat: 39.9042,
            lng: 116.4074,
            radius_meters: 200.0,
        });
        trigger.add_rule(GeofenceRule {
            zone_name: "家".into(),
            event: GeofenceEvent::Enter,
            action_description: "disabled".into(),
            enabled: false,
        });

        let rules = trigger.matching_rules(GeofenceEvent::Enter, "家");
        assert!(rules.is_empty());
    }
}
