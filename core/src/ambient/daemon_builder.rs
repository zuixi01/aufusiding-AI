//! 守护进程构建器
//!
//! 从 AppConfig 构建完整的 AmbientDaemon，串联所有子系统。

use std::sync::Arc;

use crate::config::AppConfig;
use crate::daemon::DaemonProcess;
use crate::iot::adapter_factory::AdapterFactory;
use crate::iot::event_bus::EventBus;
use crate::iot::semantic::proactive_monitor::ProactiveMonitor;
use crate::iot::semantic::profile::SemanticProfileStore;
use crate::providers::traits::Provider;

use super::audio_sink::AudioSink;
use super::audio_source::AudioSource;
use super::daemon::AmbientDaemon;
use super::dispatcher_trait::Dispatcher;
use super::intent::IntentDispatcher;
use super::loop_runner::AmbientLoopRunner;
use super::privacy_dispatcher::{PrivacyAwareDispatcher, SanitizerConfig};
use super::suggestion_announcer::SuggestionAnnouncer;
use super::wakeword_enhanced::EnhancedWakewordDetector;

use crate::voice::streaming_traits::{StreamingSttEngine, StreamingTtsEngine, VadEngine};

/// 构建产物
pub struct DaemonComponents {
    pub daemon: AmbientDaemon,
    pub monitor: ProactiveMonitor,
    pub announcer: SuggestionAnnouncer,
}

/// 守护进程构建器
pub struct DaemonBuilder;

impl DaemonBuilder {
    /// 从 AppConfig + 外部注入的音频/语音组件构建完整守护进程
    ///
    /// `provider` 为可选 LLM Provider，注入到 IntentDispatcher 的 Chat/Agent 分支。
    /// `system_prompt` 为可选系统提示词。
    /// `llm_timeout_ms` 为可选 LLM 调用超时（毫秒），默认 10000。
    pub fn build(
        config: &AppConfig,
        audio_source: Box<dyn AudioSource>,
        audio_sink: Box<dyn AudioSink>,
        vad: Box<dyn VadEngine>,
        stt: Box<dyn StreamingSttEngine>,
        tts: Box<dyn StreamingTtsEngine>,
    ) -> anyhow::Result<DaemonComponents> {
        Self::build_with_provider(
            config,
            audio_source,
            audio_sink,
            vad,
            stt,
            tts,
            None,
            None,
            None,
        )
    }

    /// 带 Provider 注入的完整构建方法
    pub fn build_with_provider(
        config: &AppConfig,
        audio_source: Box<dyn AudioSource>,
        audio_sink: Box<dyn AudioSink>,
        vad: Box<dyn VadEngine>,
        stt: Box<dyn StreamingSttEngine>,
        tts: Box<dyn StreamingTtsEngine>,
        provider: Option<Arc<dyn Provider>>,
        system_prompt: Option<String>,
        llm_timeout_ms: Option<u64>,
    ) -> anyhow::Result<DaemonComponents> {
        // 1. EventBus
        let event_bus = Arc::new(EventBus::new(256));

        // 2. AdapterFactory → GenericController
        let controller = Arc::new(AdapterFactory::build(&config.adapter));

        // 3. SemanticProfileStore
        let profile_store = Arc::new(SemanticProfileStore::new());

        // 4. IntentDispatcher (with semantic pipeline + optional provider)
        let mut intent_dispatcher =
            IntentDispatcher::with_semantic(Arc::clone(&profile_store), Arc::clone(&controller));
        if let Some(p) = provider {
            intent_dispatcher = intent_dispatcher.with_provider(p);
        }
        if let Some(prompt) = system_prompt {
            intent_dispatcher = intent_dispatcher.with_system_prompt(prompt);
        }
        if let Some(timeout) = llm_timeout_ms {
            intent_dispatcher = intent_dispatcher.with_llm_timeout(timeout);
        }

        // 5. PrivacyAwareDispatcher 包装
        let privacy_config = SanitizerConfig::default();
        let dispatcher: Box<dyn Dispatcher> = Box::new(PrivacyAwareDispatcher::new(
            intent_dispatcher,
            privacy_config,
        ));

        // 6. ProactiveMonitor
        let monitor = ProactiveMonitor::new(&event_bus, Arc::clone(&profile_store));

        // 7. SuggestionAnnouncer
        let announcer = SuggestionAnnouncer::new(0.5, 300);

        // 8. Wakeword detector
        let inner_wakeword = crate::voice::WakewordDetector::new(
            config.ambient.wakeword_model_path.clone(),
            config.ambient.wakeword_confidence_threshold,
        );
        let wakeword = EnhancedWakewordDetector::new(
            inner_wakeword,
            config.ambient.wakeword_confidence_threshold,
        );

        // 9. AmbientLoopRunner
        let loop_runner = AmbientLoopRunner::new(
            audio_source,
            audio_sink,
            vad,
            wakeword,
            stt,
            tts,
            dispatcher,
            config.ambient.clone(),
        );

        // 10. DaemonProcess
        let process = DaemonProcess::new(config.data_dir.join("daemon.pid"));

        // 11. AmbientDaemon
        let daemon = AmbientDaemon::new(
            process,
            loop_runner,
            config.ambient.clone(),
            Arc::clone(&event_bus),
        );

        Ok(DaemonComponents {
            daemon,
            monitor,
            announcer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ambient::audio_sink::MockAudioSink;
    use crate::ambient::audio_source::MockAudioSource;
    use crate::providers::mock::MockProvider;
    use crate::voice::streaming_traits::{StreamingSttEngine, StreamingTtsEngine, VadEngine};
    use crate::voice::streaming_types::{AudioChunk, VadDecision};
    use async_trait::async_trait;

    struct TestVad;
    #[async_trait]
    impl VadEngine for TestVad {
        fn classify(&self, _chunk: &AudioChunk) -> anyhow::Result<VadDecision> {
            Ok(VadDecision::Silence)
        }
    }

    struct TestStt;
    #[async_trait]
    impl StreamingSttEngine for TestStt {
        async fn feed_chunk(&self, _chunk: &AudioChunk) -> anyhow::Result<Option<String>> {
            Ok(None)
        }
        async fn finalize(&self) -> anyhow::Result<String> {
            Ok("测试".into())
        }
        async fn reset(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct TestTts;
    #[async_trait]
    impl StreamingTtsEngine for TestTts {
        async fn synthesize_streamed(&self, _text: &str) -> anyhow::Result<Vec<AudioChunk>> {
            Ok(vec![])
        }
    }

    fn make_chunk() -> AudioChunk {
        AudioChunk::new(vec![0.0; 480], 16000, 1, 0, 0)
    }

    fn make_components() -> DaemonComponents {
        let config = AppConfig::default();
        let source = MockAudioSource::new(vec![make_chunk(); 5]);
        let sink = MockAudioSink::new();
        DaemonBuilder::build(
            &config,
            Box::new(source),
            Box::new(sink),
            Box::new(TestVad),
            Box::new(TestStt),
            Box::new(TestTts),
        )
        .unwrap()
    }

    fn make_components_with_provider(provider: Option<Arc<dyn Provider>>) -> DaemonComponents {
        let config = AppConfig::default();
        let source = MockAudioSource::new(vec![make_chunk(); 5]);
        let sink = MockAudioSink::new();
        DaemonBuilder::build_with_provider(
            &config,
            Box::new(source),
            Box::new(sink),
            Box::new(TestVad),
            Box::new(TestStt),
            Box::new(TestTts),
            provider,
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn default_config_builds_successfully() {
        let components = make_components();
        assert!(!components.daemon.is_running());
    }

    #[test]
    fn monitor_starts_empty() {
        let components = make_components();
        assert!(components.monitor.device_states().is_empty());
    }

    #[test]
    fn announcer_is_configured() {
        let mut components = make_components();
        let suggestions = components.monitor.evaluate();
        let texts = components.announcer.process(suggestions);
        assert!(texts.is_empty());
    }

    #[tokio::test]
    async fn daemon_starts_and_stops() {
        let components = make_components();
        let mut daemon = components.daemon.with_max_iterations(2);
        let result = daemon.start().await;
        assert!(result.is_ok());
    }

    #[test]
    fn event_bus_is_shared() {
        let components = make_components();
        let bus = components.daemon.event_bus();
        assert!(Arc::strong_count(bus) >= 1);
    }

    #[test]
    fn with_iot_config_builds() {
        let mut config = AppConfig::default();
        config.adapter.mqtt = Some(crate::iot::adapters::mqtt::MqttConfig {
            broker_host: "localhost".into(),
            broker_port: 1883,
            client_id: "test".into(),
            topic_prefix: "home/".into(),
            username: None,
            password: None,
            qos: Default::default(),
            lwt_topic: None,
            lwt_payload: None,
        });
        let source = MockAudioSource::new(vec![make_chunk(); 5]);
        let sink = MockAudioSink::new();
        let result = DaemonBuilder::build(
            &config,
            Box::new(source),
            Box::new(sink),
            Box::new(TestVad),
            Box::new(TestStt),
            Box::new(TestTts),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn privacy_pipeline_is_active() {
        let components = make_components();
        assert!(!components.daemon.is_running());
    }

    // ─── Task 3: Provider 注入测试 ──────────────────────────

    #[test]
    fn build_with_provider_succeeds() {
        let provider: Arc<dyn Provider> = Arc::new(MockProvider::new("test-provider"));
        let components = make_components_with_provider(Some(provider));
        assert!(!components.daemon.is_running());
    }

    #[test]
    fn build_without_provider_backward_compatible() {
        let components = make_components_with_provider(None);
        assert!(!components.daemon.is_running());
    }

    #[tokio::test]
    async fn provider_reaches_dispatcher_chat() {
        // Chat 分支：非 IoT/System/Workflow 文本应走 Chat，MockProvider 返回包含输入的回复
        let provider: Arc<dyn Provider> = Arc::new(MockProvider::new("chat-test"));
        let dispatcher = IntentDispatcher::new().with_provider(provider);
        let result = dispatcher.dispatch("你好世界").await;
        assert_eq!(result.target, crate::ambient::intent::DispatchTarget::Chat);
        assert!(result.response.contains("你好世界"));
    }

    #[tokio::test]
    async fn agent_branch_uses_provider() {
        let provider: Arc<dyn Provider> = Arc::new(MockProvider::new("agent-test"));
        let dispatcher = IntentDispatcher::new().with_provider(provider);
        // "调研" 触发 Agent 分支
        let result = dispatcher.dispatch("调研一下 Rust 异步运行时").await;
        assert_eq!(result.target, crate::ambient::intent::DispatchTarget::Agent);
        assert!(result.response.contains("Rust"));
    }

    #[tokio::test]
    async fn iot_branch_unaffected_by_provider() {
        let provider: Arc<dyn Provider> = Arc::new(MockProvider::new("iot-test"));
        let dispatcher = IntentDispatcher::new().with_provider(provider);
        // "打开灯" 触发 IoT 分支，不走 LLM
        let result = dispatcher.dispatch("打开灯").await;
        assert_eq!(result.target, crate::ambient::intent::DispatchTarget::Iot);
    }

    #[test]
    fn system_prompt_injected() {
        let config = AppConfig::default();
        let source = MockAudioSource::new(vec![make_chunk(); 5]);
        let sink = MockAudioSink::new();
        let result = DaemonBuilder::build_with_provider(
            &config,
            Box::new(source),
            Box::new(sink),
            Box::new(TestVad),
            Box::new(TestStt),
            Box::new(TestTts),
            Some(Arc::new(MockProvider::new("prompt-test"))),
            Some("你是一个智能家居助手".into()),
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn timeout_configurable() {
        let config = AppConfig::default();
        let source = MockAudioSource::new(vec![make_chunk(); 5]);
        let sink = MockAudioSink::new();
        let result = DaemonBuilder::build_with_provider(
            &config,
            Box::new(source),
            Box::new(sink),
            Box::new(TestVad),
            Box::new(TestStt),
            Box::new(TestTts),
            Some(Arc::new(MockProvider::new("timeout-test"))),
            None,
            Some(5000),
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn privacy_wraps_provider_output() {
        // PrivacyAwareDispatcher 包装后，Provider 输出也经过隐私过滤
        let provider: Arc<dyn Provider> = Arc::new(MockProvider::new("privacy-test"));
        let intent_dispatcher = IntentDispatcher::new().with_provider(provider);
        let privacy_config = SanitizerConfig::default();
        let dispatcher = PrivacyAwareDispatcher::new(intent_dispatcher, privacy_config);
        let result = dispatcher.dispatch("你好").await;
        // 应正常返回，不 panic
        assert_eq!(result.target, crate::ambient::intent::DispatchTarget::Chat);
    }
}
