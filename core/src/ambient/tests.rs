//! 环境计算模块测试

#[cfg(test)]
mod unit {
    use crate::ambient::*;
    use crate::voice::streaming_types::{AudioChunk, VadDecision};
    use crate::voice::streaming_traits::{StreamingSttEngine, StreamingTtsEngine, VadEngine};
    use async_trait::async_trait;

    // ─── Mock 引擎 ──────────────────────────────────────

    struct MockVad {
        decisions: std::sync::Mutex<Vec<VadDecision>>,
        index: std::sync::atomic::AtomicUsize,
    }
    impl MockVad {
        fn new(decisions: Vec<VadDecision>) -> Self {
            Self {
                decisions: std::sync::Mutex::new(decisions),
                index: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }
    impl VadEngine for MockVad {
        fn classify(&self, _chunk: &AudioChunk) -> anyhow::Result<VadDecision> {
            let idx = self.index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let decs = self.decisions.lock().unwrap();
            Ok(decs.get(idx).copied().unwrap_or(VadDecision::Silence))
        }
    }

    struct MockStt {
        transcript: std::sync::Mutex<String>,
    }
    impl MockStt {
        fn new(text: &str) -> Self {
            Self { transcript: std::sync::Mutex::new(text.to_string()) }
        }
    }
    #[async_trait]
    impl StreamingSttEngine for MockStt {
        async fn feed_chunk(&self, _chunk: &AudioChunk) -> anyhow::Result<Option<String>> {
            Ok(None)
        }
        async fn finalize(&self) -> anyhow::Result<String> {
            Ok(self.transcript.lock().unwrap().clone())
        }
        async fn reset(&self) -> anyhow::Result<()> { Ok(()) }
    }

    struct MockTts;
    #[async_trait]
    impl StreamingTtsEngine for MockTts {
        async fn synthesize_streamed(&self, _text: &str) -> anyhow::Result<Vec<AudioChunk>> {
            Ok(vec![AudioChunk::new(vec![0.0; 480], 16000, 1, 0, 0)])
        }
    }

    fn make_speech_chunk(seq: u64) -> AudioChunk {
        let samples: Vec<f32> = (0..480).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        AudioChunk::new(samples, 16000, 1, seq, seq * 30)
    }

    fn make_silence_chunk(seq: u64) -> AudioChunk {
        AudioChunk::new(vec![0.0; 480], 16000, 1, seq, seq * 30)
    }

    fn make_wakeword_detector() -> EnhancedWakewordDetector {
        let inner = crate::voice::WakewordDetector::new("".into(), 0.05);
        EnhancedWakewordDetector::new(inner, 0.3) // 低阈值方便测试
    }

    fn make_runner(
        vad_decisions: Vec<VadDecision>,
        stt_text: &str,
    ) -> AmbientLoopRunner {
        let source = MockAudioSource::new(
            (0..100).map(|i| make_speech_chunk(i)).collect()
        );
        let sink = MockAudioSink::new();
        let vad = MockVad::new(vad_decisions);
        let wakeword = make_wakeword_detector();
        let stt = MockStt::new(stt_text);
        let tts = MockTts;
        let dispatcher: Box<dyn crate::ambient::dispatcher_trait::Dispatcher> =
            Box::new(IntentDispatcher::new());
        let config = AmbientConfig::default();

        AmbientLoopRunner::new(
            Box::new(source),
            Box::new(sink),
            Box::new(vad),
            wakeword,
            Box::new(stt),
            Box::new(tts),
            dispatcher,
            config,
        )
    }

    // ─── 类型测试 ──────────────────────────────────────

    #[test]
    fn ambient_state_idle_is_not_active() {
        assert!(!AmbientState::Idle.is_active());
    }

    #[test]
    fn ambient_state_listening_is_active() {
        assert!(AmbientState::Listening.is_active());
        assert!(AmbientState::Processing.is_active());
        assert!(AmbientState::Speaking.is_active());
        assert!(AmbientState::Conversation.is_active());
        assert!(AmbientState::WakewordDetected.is_active());
    }

    #[test]
    fn ambient_state_serde_roundtrip() {
        let state = AmbientState::Listening;
        let json = serde_json::to_string(&state).unwrap();
        let back: AmbientState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }

    #[test]
    fn listening_mode_serde_roundtrip() {
        let mode = ListeningMode::Continuous;
        let json = serde_json::to_string(&mode).unwrap();
        let back: ListeningMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }

    #[test]
    fn conversation_turn_creation() {
        let turn = ConversationTurn::new("你好".into(), "你好！".into());
        assert_eq!(turn.user_text, "你好");
        assert_eq!(turn.response_text, "你好！");
        assert!(turn.speaker_id.is_none());
    }

    // ─── 配置测试 ──────────────────────────────────────

    #[test]
    fn default_config_values() {
        let cfg = AmbientConfig::default();
        assert_eq!(cfg.wakeword, "小普");
        assert_eq!(cfg.wakeword_confidence_threshold, 0.85);
        assert_eq!(cfg.post_wakeword_timeout_ms, 3000);
        assert_eq!(cfg.silence_timeout_ms, 1500);
        assert_eq!(cfg.conversation_timeout_ms, 30000);
        assert_eq!(cfg.max_conversation_turns, 10);
        assert_eq!(cfg.sample_rate, 16000);
        assert_eq!(cfg.channels, 1);
        assert!(!cfg.local_only);
        assert!(!cfg.voiceprint_enabled);
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = AmbientConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AmbientConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.wakeword, "小普");
        assert_eq!(back.sample_rate, 16000);
    }

    // ─── 意图分发测试 ──────────────────────────────────

    #[test]
    fn intent_classify_iot() {
        let d = IntentDispatcher::new();
        assert_eq!(d.classify("打开客厅灯"), DispatchTarget::Iot);
        assert_eq!(d.classify("关闭空调"), DispatchTarget::Iot);
        assert_eq!(d.classify("把温度调高一点"), DispatchTarget::Iot);
    }

    #[test]
    fn intent_classify_system() {
        let d = IntentDispatcher::new();
        assert_eq!(d.classify("音量调大"), DispatchTarget::System);
        assert_eq!(d.classify("静音"), DispatchTarget::System);
    }

    #[test]
    fn intent_classify_workflow() {
        let d = IntentDispatcher::new();
        assert_eq!(d.classify("每天早上7点提醒我"), DispatchTarget::Workflow);
        assert_eq!(d.classify("设个闹钟"), DispatchTarget::Workflow);
    }

    #[test]
    fn intent_classify_agent() {
        let d = IntentDispatcher::new();
        assert_eq!(d.classify("帮我调研比亚迪股票"), DispatchTarget::Agent);
        assert_eq!(d.classify("分析一下这个数据"), DispatchTarget::Agent);
    }

    #[test]
    fn intent_classify_chat_default() {
        let d = IntentDispatcher::new();
        assert_eq!(d.classify("今天天气怎么样"), DispatchTarget::Chat);
        assert_eq!(d.classify("你好"), DispatchTarget::Chat);
    }

    #[tokio::test]
    async fn intent_dispatch_returns_result() {
        use std::sync::Arc;
        use crate::iot::control::{GenericController, MockDeviceController};
        use crate::iot::semantic::profile::{build_default_profile, SemanticProfileStore};
        use crate::iot::types::DeviceType;

        let mut store = SemanticProfileStore::new();
        store.add(build_default_profile("light-001", "客厅灯", DeviceType::Light, Some("客厅")));
        let store = Arc::new(store);

        let mut gc = GenericController::new();
        let mock = MockDeviceController::new("Light");
        mock.register_device("light-001", "Light");
        gc.register_adapter(Box::new(mock));
        let gc = Arc::new(gc);

        let d = IntentDispatcher::with_semantic(store, gc);
        let result = d.dispatch("打开客厅灯").await;
        assert_eq!(result.target, DispatchTarget::Iot);
        assert!(result.is_action);
        assert!(!result.response.is_empty());
    }

    // ─── 响应格式化测试 ────────────────────────────────

    #[test]
    fn format_empty_action_result() {
        let f = ResponseFormatter::new();
        assert_eq!(f.format("", true), "已完成。");
    }

    #[test]
    fn format_empty_query_result() {
        let f = ResponseFormatter::new();
        assert_eq!(f.format("", false), "抱歉，我没有获取到结果。");
    }

    #[test]
    fn format_action_adds_prefix() {
        let f = ResponseFormatter::new();
        let result = f.format("灯已打开", true);
        assert!(result.starts_with("好的，"));
    }

    #[test]
    fn format_strips_markdown() {
        let f = ResponseFormatter::new();
        let result = f.format("**重要** 内容", false);
        assert!(!result.contains("**"));
    }

    #[test]
    fn format_truncates_long_text() {
        let f = ResponseFormatter::new().with_max_chars(20);
        let long_text = "这是一段很长的文本，需要被截断处理，因为语音输出不适合太长的内容。";
        let result = f.format(long_text, false);
        assert!(result.chars().count() <= 30); // 允许截断后缀
    }

    // ─── 房间上下文测试 ────────────────────────────────

    #[test]
    fn room_context_creation() {
        let ctx = RoomContext::new("客厅".into(), "device-1".into());
        assert_eq!(ctx.current_room, "客厅");
        assert_eq!(ctx.device_id, "device-1");
        assert_eq!(ctx.known_rooms.len(), 1);
    }

    #[test]
    fn room_context_update_room() {
        let mut ctx = RoomContext::new("客厅".into(), "device-1".into());
        ctx.update_room("卧室");
        assert_eq!(ctx.current_room, "卧室");
        assert_eq!(ctx.known_rooms.len(), 2);
    }

    #[test]
    fn room_context_register_device() {
        let mut ctx = RoomContext::new("客厅".into(), "device-1".into());
        ctx.register_device("客厅", "speaker-1");
        ctx.register_device("客厅", "light-1");
        let devices = ctx.current_room_devices();
        assert_eq!(devices.len(), 2);
        assert!(devices.contains(&"speaker-1"));
    }

    #[test]
    fn room_context_no_duplicate_devices() {
        let mut ctx = RoomContext::new("客厅".into(), "device-1".into());
        ctx.register_device("客厅", "speaker-1");
        ctx.register_device("客厅", "speaker-1");
        assert_eq!(ctx.current_room_devices().len(), 1);
    }

    // ─── 增强唤醒词测试 ────────────────────────────────

    #[test]
    fn enhanced_wakeword_low_confidence_rejected() {
        let inner = crate::voice::WakewordDetector::new("".into(), 0.05);
        let mut detector = EnhancedWakewordDetector::new(inner, 0.99);
        let audio = crate::voice::AudioBuffer::silence(1.0, 16000, 1);
        let result = detector.detect_with_confidence(&audio).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn enhanced_wakeword_requires_followup_default() {
        let inner = crate::voice::WakewordDetector::new("".into(), 0.05);
        let detector = EnhancedWakewordDetector::new(inner, 0.85);
        assert!(detector.requires_followup());
    }

    #[test]
    fn enhanced_wakeword_followup_configurable() {
        let inner = crate::voice::WakewordDetector::new("".into(), 0.05);
        let detector = EnhancedWakewordDetector::new(inner, 0.85)
            .with_followup_required(false);
        assert!(!detector.requires_followup());
    }

    // ─── MockAudioSource/Sink 测试 ─────────────────────

    #[tokio::test]
    async fn mock_audio_source_returns_chunks() {
        let chunks = vec![make_speech_chunk(0), make_speech_chunk(1)];
        let source = MockAudioSource::new(chunks);
        let c1 = source.read_chunk().await.unwrap();
        assert_eq!(c1.sequence_id, 0);
        let c2 = source.read_chunk().await.unwrap();
        assert_eq!(c2.sequence_id, 1);
    }

    #[tokio::test]
    async fn mock_audio_source_returns_silence_when_exhausted() {
        let source = MockAudioSource::new(vec![make_speech_chunk(0)]);
        let _ = source.read_chunk().await.unwrap();
        let c = source.read_chunk().await.unwrap();
        assert!(c.samples.iter().all(|&s| s == 0.0));
    }

    #[tokio::test]
    async fn mock_audio_sink_records_played() {
        let sink = MockAudioSink::new();
        let chunk = make_speech_chunk(0);
        sink.play_chunk(&chunk).await.unwrap();
        assert_eq!(sink.played_count(), 1);
    }

    // ─── 主循环状态机测试 ──────────────────────────────

    #[tokio::test]
    async fn loop_runner_starts_idle() {
        let runner = make_runner(vec![VadDecision::Silence], "");
        assert_eq!(runner.state(), AmbientState::Idle);
    }

    #[tokio::test]
    async fn loop_runner_stays_idle_on_silence() {
        let mut runner = make_runner(
            vec![VadDecision::Silence, VadDecision::Silence],
            "",
        );
        runner.tick().await.unwrap();
        assert_eq!(runner.state(), AmbientState::Idle);
    }

    #[tokio::test]
    async fn loop_runner_reset_returns_to_idle() {
        let mut runner = make_runner(vec![], "");
        runner.set_listening_mode(ListeningMode::Continuous);
        runner.reset();
        assert_eq!(runner.state(), AmbientState::Idle);
        assert!(runner.conversation_turns().is_empty());
    }

    #[tokio::test]
    async fn loop_runner_listening_mode_configurable() {
        let mut runner = make_runner(vec![], "");
        assert_eq!(runner.listening_mode(), ListeningMode::WakewordRequired);
        runner.set_listening_mode(ListeningMode::Continuous);
        assert_eq!(runner.listening_mode(), ListeningMode::Continuous);
    }

    #[tokio::test]
    async fn loop_runner_continuous_mode_enters_listening_on_speech() {
        let mut runner = make_runner(
            vec![VadDecision::Speech],
            "你好",
        );
        runner.set_listening_mode(ListeningMode::Continuous);
        runner.tick().await.unwrap();
        assert_eq!(runner.state(), AmbientState::Listening);
    }

    // ─── Daemon 测试 ───────────────────────────────────

    #[tokio::test]
    async fn daemon_creation() {
        let process = crate::daemon::DaemonProcess::new(
            crate::test_support::temp_file_path("ambient-daemon", "pid"),
        );
        let runner = make_runner(vec![VadDecision::Silence], "");
        let config = AmbientConfig::default();
        let event_bus = std::sync::Arc::new(crate::iot::EventBus::new(16));
        let daemon = AmbientDaemon::new(process, runner, config, event_bus.clone());
        assert!(!daemon.is_running());
        assert_eq!(daemon.config().wakeword, "小普");
    }

    #[tokio::test]
    async fn daemon_start_stop() {
        let process = crate::daemon::DaemonProcess::new(
            crate::test_support::temp_file_path("ambient-daemon-ss", "pid"),
        );
        let runner = make_runner(vec![VadDecision::Silence], "");
        let config = AmbientConfig::default();
        let event_bus = std::sync::Arc::new(crate::iot::EventBus::new(16));
        let mut daemon = AmbientDaemon::new(process, runner, config, event_bus)
            .with_max_iterations(2);
        daemon.start().await.unwrap();
        assert!(daemon.is_running());
        daemon.stop().await.unwrap();
        assert!(!daemon.is_running());
    }

    // ─── LLM Provider 集成测试 ──────────────────────────

    #[tokio::test]
    async fn dispatch_chat_with_provider_returns_llm_response() {
        use crate::providers::mock::MockProvider;
        let provider = std::sync::Arc::new(MockProvider::new("test-llm"));
        let d = IntentDispatcher::new().with_provider(provider);
        let result = d.dispatch("今天天气怎么样").await;
        assert_eq!(result.target, DispatchTarget::Chat);
        assert!(result.response.contains("[mock:test-llm]"));
    }

    #[tokio::test]
    async fn dispatch_agent_with_provider_returns_llm_response() {
        use crate::providers::mock::MockProvider;
        let provider = std::sync::Arc::new(MockProvider::new("test-llm"));
        let d = IntentDispatcher::new().with_provider(provider);
        let result = d.dispatch("帮我调研量子计算").await;
        assert_eq!(result.target, DispatchTarget::Agent);
        assert!(result.response.contains("[mock:test-llm]"));
    }

    #[tokio::test]
    async fn dispatch_chat_without_provider_fallback() {
        let d = IntentDispatcher::new();
        let result = d.dispatch("今天天气怎么样").await;
        assert_eq!(result.target, DispatchTarget::Chat);
        assert!(result.response.contains("收到您的问题"));
    }

    #[tokio::test]
    async fn dispatch_with_system_prompt() {
        use crate::providers::mock::MockProvider;
        let provider = std::sync::Arc::new(MockProvider::new("test-llm"));
        let d = IntentDispatcher::new()
            .with_provider(provider)
            .with_system_prompt("你是普罗米修斯AI助手".into());
        let result = d.dispatch("你好").await;
        assert!(result.response.contains("[mock:test-llm]"));
    }

    #[tokio::test]
    async fn dispatch_iot_unaffected_by_provider() {
        use crate::providers::mock::MockProvider;
        let provider = std::sync::Arc::new(MockProvider::new("test-llm"));
        let d = IntentDispatcher::new().with_provider(provider);
        let result = d.dispatch("打开客厅灯").await;
        assert_eq!(result.target, DispatchTarget::Iot);
        // IoT 分支不走 LLM
        assert!(!result.response.contains("[mock:test-llm]"));
    }

    #[tokio::test]
    async fn dispatch_mock_provider_deterministic() {
        use crate::providers::mock::MockProvider;
        let provider = std::sync::Arc::new(MockProvider::new("det"));
        let d = IntentDispatcher::new().with_provider(provider);
        let r1 = d.dispatch("你好").await;
        let r2 = d.dispatch("你好").await;
        assert_eq!(r1.response, r2.response);
    }

    #[tokio::test]
    async fn dispatch_provider_error_fallback() {
        use crate::providers::traits::{ChatRequest, ChatResponse, Provider, TokenUsage};

        struct FailProvider;
        #[async_trait]
        impl Provider for FailProvider {
            fn name(&self) -> &str { "fail" }
            fn available_models(&self) -> Vec<String> { vec!["fail-model".into()] }
            async fn chat(&self, _: &ChatRequest) -> anyhow::Result<ChatResponse> {
                anyhow::bail!("模拟错误")
            }
            async fn embed(&self, _: &str) -> anyhow::Result<Vec<f32>> { Ok(vec![]) }
            async fn validate_key(&self) -> anyhow::Result<bool> { Ok(false) }
        }

        let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FailProvider);
        let d = IntentDispatcher::new().with_provider(provider);
        let result = d.dispatch("你好").await;
        assert_eq!(result.target, DispatchTarget::Chat);
        assert!(result.response.contains("收到您的问题"));
    }

    #[tokio::test]
    async fn dispatch_timeout_fallback() {
        use crate::providers::traits::{ChatRequest, ChatResponse, Provider, TokenUsage};

        struct SlowProvider;
        #[async_trait]
        impl Provider for SlowProvider {
            fn name(&self) -> &str { "slow" }
            fn available_models(&self) -> Vec<String> { vec!["slow-model".into()] }
            async fn chat(&self, _: &ChatRequest) -> anyhow::Result<ChatResponse> {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                unreachable!()
            }
            async fn embed(&self, _: &str) -> anyhow::Result<Vec<f32>> { Ok(vec![]) }
            async fn validate_key(&self) -> anyhow::Result<bool> { Ok(false) }
        }

        let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(SlowProvider);
        let d = IntentDispatcher::new()
            .with_provider(provider)
            .with_llm_timeout(50); // 50ms 超时
        let result = d.dispatch("你好").await;
        assert_eq!(result.target, DispatchTarget::Chat);
        assert!(result.response.contains("收到您的问题"));
    }

    #[tokio::test]
    async fn privacy_dispatcher_with_llm_provider() {
        use crate::providers::mock::MockProvider;
        use crate::ambient::privacy_dispatcher::{PrivacyAwareDispatcher, SanitizerConfig};
        use crate::ambient::dispatcher_trait::Dispatcher;

        let provider = std::sync::Arc::new(MockProvider::new("priv-test"));
        let inner = IntentDispatcher::new().with_provider(provider);
        let dispatcher = PrivacyAwareDispatcher::new(inner, SanitizerConfig::default());
        let result = dispatcher.dispatch("你好").await;
        assert!(result.response.contains("[mock:priv-test]"));
    }
}
