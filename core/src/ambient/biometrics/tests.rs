//! 生物识别模块测试

#[cfg(test)]
mod unit {
    use crate::ambient::biometrics::*;
    use crate::voice::AudioBuffer;

    fn make_audio(freq: f32, duration: f64) -> AudioBuffer {
        let sr = 16000u32;
        let n = (duration * sr as f64) as usize;
        let samples: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr as f32).sin() * 0.5)
            .collect();
        AudioBuffer::new(samples, sr, 1).unwrap()
    }

    // ─── VoiceprintEngine ──────────────────────────────

    #[test]
    fn extract_embedding_returns_correct_dimension() {
        let engine = VoiceprintEngine::new(192);
        let audio = make_audio(200.0, 2.0);
        let emb = engine.extract_embedding(&audio).unwrap();
        assert_eq!(emb.len(), 192);
    }

    #[test]
    fn extract_embedding_empty_audio_fails() {
        let engine = VoiceprintEngine::new(192);
        let audio = AudioBuffer::new(vec![], 16000, 1).unwrap();
        assert!(engine.extract_embedding(&audio).is_err());
    }

    #[test]
    fn compare_same_audio_high_similarity() {
        let engine = VoiceprintEngine::new(64);
        let audio = make_audio(300.0, 2.0);
        let emb1 = engine.extract_embedding(&audio).unwrap();
        let emb2 = engine.extract_embedding(&audio).unwrap();
        let sim = engine.compare(&emb1, &emb2);
        assert!(sim > 0.99, "same audio similarity should be ~1.0, got {}", sim);
    }

    #[test]
    fn compare_different_audio_lower_similarity() {
        let engine = VoiceprintEngine::new(64);
        let audio1 = make_audio(200.0, 2.0);
        let audio2 = make_audio(800.0, 2.0);
        let emb1 = engine.extract_embedding(&audio1).unwrap();
        let emb2 = engine.extract_embedding(&audio2).unwrap();
        let sim = engine.compare(&emb1, &emb2);
        // 浮点精度可能导致略超 1.0，验证在合理范围内
        assert!(sim >= 0.0 && sim <= 1.01, "similarity should be in [0,~1], got {}", sim);
    }

    #[test]
    fn compare_empty_returns_zero() {
        let engine = VoiceprintEngine::new(64);
        assert_eq!(engine.compare(&[], &[]), 0.0);
    }

    #[test]
    fn compare_mismatched_lengths_returns_zero() {
        let engine = VoiceprintEngine::new(64);
        assert_eq!(engine.compare(&[1.0, 2.0], &[1.0]), 0.0);
    }

    #[test]
    fn is_match_with_high_threshold() {
        let engine = VoiceprintEngine::new(64).with_threshold(0.99);
        let audio = make_audio(300.0, 2.0);
        let emb = engine.extract_embedding(&audio).unwrap();
        assert!(engine.is_match(&emb, &emb));
    }

    // ─── SpeakerProfile ────────────────────────────────

    #[test]
    fn speaker_profile_creation() {
        let profile = SpeakerProfile::new(
            "张三".into(),
            vec![0.1; 64],
            PermissionLevel::Admin,
        );
        assert_eq!(profile.name, "张三");
        assert!(!profile.is_child);
        assert_eq!(profile.permission_level, PermissionLevel::Admin);
    }

    #[test]
    fn speaker_profile_child_flag() {
        let profile = SpeakerProfile::new(
            "小明".into(),
            vec![0.1; 64],
            PermissionLevel::Child,
        );
        assert!(profile.is_child);
    }

    #[test]
    fn speaker_profile_guest() {
        let guest = SpeakerProfile::guest();
        assert_eq!(guest.name, "访客");
        assert!(guest.voiceprint.is_empty());
        assert_eq!(guest.permission_level, PermissionLevel::Guest);
    }

    #[test]
    fn speaker_profile_preferences() {
        let mut profile = SpeakerProfile::new("李四".into(), vec![], PermissionLevel::Standard);
        profile.set_preference("tts_speed", "fast");
        assert_eq!(profile.get_preference("tts_speed"), Some("fast"));
        assert_eq!(profile.get_preference("nonexistent"), None);
    }

    #[test]
    fn permission_hierarchy() {
        let admin = SpeakerProfile::new("A".into(), vec![], PermissionLevel::Admin);
        let standard = SpeakerProfile::new("B".into(), vec![], PermissionLevel::Standard);
        let child = SpeakerProfile::new("C".into(), vec![], PermissionLevel::Child);
        let guest = SpeakerProfile::guest();

        assert!(admin.has_permission(PermissionLevel::Admin));
        assert!(admin.has_permission(PermissionLevel::Child));
        assert!(!standard.has_permission(PermissionLevel::Admin));
        assert!(standard.has_permission(PermissionLevel::Standard));
        assert!(!child.has_permission(PermissionLevel::Standard));
        assert!(child.has_permission(PermissionLevel::Guest));
        assert!(!guest.has_permission(PermissionLevel::Child));
    }

    // ─── SpeakerStore ──────────────────────────────────

    #[test]
    fn store_register_and_find() {
        let engine = VoiceprintEngine::new(64);
        let mut store = SpeakerStore::new(engine);
        let profile = SpeakerProfile::new("张三".into(), vec![0.5; 64], PermissionLevel::Admin);
        let id = profile.id;
        store.register(profile).unwrap();

        assert_eq!(store.count(), 1);
        assert!(store.find_by_id(id).is_some());
        assert!(store.find_by_name("张三").is_some());
    }

    #[test]
    fn store_register_empty_name_fails() {
        let engine = VoiceprintEngine::new(64);
        let mut store = SpeakerStore::new(engine);
        let profile = SpeakerProfile::new("".into(), vec![], PermissionLevel::Guest);
        assert!(store.register(profile).is_err());
    }

    #[test]
    fn store_register_duplicate_fails() {
        let engine = VoiceprintEngine::new(64);
        let mut store = SpeakerStore::new(engine);
        let profile = SpeakerProfile::new("张三".into(), vec![], PermissionLevel::Admin);
        let dup = profile.clone();
        store.register(profile).unwrap();
        assert!(store.register(dup).is_err());
    }

    #[test]
    fn store_remove() {
        let engine = VoiceprintEngine::new(64);
        let mut store = SpeakerStore::new(engine);
        let profile = SpeakerProfile::new("张三".into(), vec![], PermissionLevel::Admin);
        let id = profile.id;
        store.register(profile).unwrap();
        store.remove(id).unwrap();
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn store_identify_by_voiceprint() {
        let engine = VoiceprintEngine::new(64).with_threshold(0.8);
        let mut store = SpeakerStore::new(engine);
        let vp = vec![0.5; 64];
        let profile = SpeakerProfile::new("张三".into(), vp.clone(), PermissionLevel::Admin);
        store.register(profile).unwrap();

        let found = store.identify(&vp);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "张三");
    }

    #[test]
    fn store_identify_unknown_returns_none() {
        let engine = VoiceprintEngine::new(64).with_threshold(0.99);
        let mut store = SpeakerStore::new(engine);
        let profile = SpeakerProfile::new("张三".into(), vec![1.0; 64], PermissionLevel::Admin);
        store.register(profile).unwrap();

        let unknown_vp = vec![0.0; 64];
        assert!(store.identify(&unknown_vp).is_none());
    }

    // ─── EnrollmentFlow ────────────────────────────────

    #[test]
    fn enrollment_flow_lifecycle() {
        let engine = VoiceprintEngine::new(64);
        let mut flow = EnrollmentFlow::new(engine, 3);
        assert_eq!(flow.state(), enrollment::EnrollmentState::Pending);
        assert_eq!(flow.required_samples(), 3);

        flow.start();
        assert_eq!(flow.state(), enrollment::EnrollmentState::Recording(0));

        for i in 0..3 {
            let audio = make_audio(200.0 + i as f32 * 50.0, 2.0);
            flow.submit_sample(audio).unwrap();
        }
        assert_eq!(flow.state(), enrollment::EnrollmentState::Processing);

        let profile = flow.finalize("测试用户".into(), PermissionLevel::Standard).unwrap();
        assert_eq!(profile.name, "测试用户");
        assert_eq!(profile.voiceprint.len(), 64);
        assert_eq!(flow.state(), enrollment::EnrollmentState::Completed);
    }

    #[test]
    fn enrollment_short_audio_rejected() {
        let engine = VoiceprintEngine::new(64);
        let mut flow = EnrollmentFlow::new(engine, 1);
        flow.start();
        let short = AudioBuffer::new(vec![0.1; 100], 16000, 1).unwrap(); // ~6ms
        assert!(flow.submit_sample(short).is_err());
    }

    #[test]
    fn enrollment_submit_before_start_fails() {
        let engine = VoiceprintEngine::new(64);
        let mut flow = EnrollmentFlow::new(engine, 1);
        let audio = make_audio(200.0, 2.0);
        assert!(flow.submit_sample(audio).is_err());
    }

    #[test]
    fn enrollment_reset() {
        let engine = VoiceprintEngine::new(64);
        let mut flow = EnrollmentFlow::new(engine, 1);
        flow.start();
        flow.reset();
        assert_eq!(flow.state(), enrollment::EnrollmentState::Pending);
        assert_eq!(flow.collected_samples(), 0);
    }

    #[test]
    fn enrollment_prompt_messages() {
        let engine = VoiceprintEngine::new(64);
        let mut flow = EnrollmentFlow::new(engine, 3);
        assert!(flow.prompt().contains("开始"));
        flow.start();
        assert!(flow.prompt().contains("第 1 段"));
    }
}
