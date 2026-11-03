//! 环境主循环运行器
//!
//! `AmbientLoopRunner` — tokio 任务：音频采集 → VAD → 唤醒词 → STT → 分发 → TTS

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::voice::streaming_traits::{StreamingSttEngine, StreamingTtsEngine, VadEngine};
use crate::voice::streaming_types::VadDecision;

use super::audio_sink::AudioSink;
use super::audio_source::AudioSource;
use super::config::AmbientConfig;
use super::dispatcher_trait::Dispatcher;
use super::intent::DispatchResult;
use super::response::ResponseFormatter;
use super::room_context::RoomContext;
use super::types::{AmbientState, ConversationTurn, ListeningMode};
use super::wakeword_enhanced::EnhancedWakewordDetector;

/// 环境主循环运行器
pub struct AmbientLoopRunner {
    audio_source: Box<dyn AudioSource>,
    audio_sink: Box<dyn AudioSink>,
    vad: Box<dyn VadEngine>,
    wakeword: Arc<Mutex<EnhancedWakewordDetector>>,
    stt: Box<dyn StreamingSttEngine>,
    tts: Box<dyn StreamingTtsEngine>,
    dispatcher: Box<dyn Dispatcher>,
    formatter: ResponseFormatter,
    room: RoomContext,
    state: AmbientState,
    config: AmbientConfig,
    listening_mode: ListeningMode,
    conversation_turns: Vec<ConversationTurn>,
    /// 连续静音块计数
    silence_count: u64,
    /// 连续语音块计数
    speech_count: u64,
    /// 最近一次分发结果
    last_result: Option<DispatchResult>,
}

impl AmbientLoopRunner {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        audio_source: Box<dyn AudioSource>,
        audio_sink: Box<dyn AudioSink>,
        vad: Box<dyn VadEngine>,
        wakeword: EnhancedWakewordDetector,
        stt: Box<dyn StreamingSttEngine>,
        tts: Box<dyn StreamingTtsEngine>,
        dispatcher: Box<dyn Dispatcher>,
        config: AmbientConfig,
    ) -> Self {
        let room = RoomContext::new(config.default_room.clone(), config.device_id.clone());
        Self {
            audio_source,
            audio_sink,
            vad,
            wakeword: Arc::new(Mutex::new(wakeword)),
            stt,
            tts,
            dispatcher,
            formatter: ResponseFormatter::new(),
            room,
            state: AmbientState::Idle,
            config,
            listening_mode: ListeningMode::WakewordRequired,
            conversation_turns: Vec::new(),
            silence_count: 0,
            speech_count: 0,
            last_result: None,
        }
    }

    /// 获取当前状态
    pub fn state(&self) -> AmbientState {
        self.state
    }

    /// 设置监听模式
    pub fn set_listening_mode(&mut self, mode: ListeningMode) {
        self.listening_mode = mode;
    }

    /// 获取监听模式
    pub fn listening_mode(&self) -> ListeningMode {
        self.listening_mode
    }

    /// 获取房间上下文
    pub fn room(&self) -> &RoomContext {
        &self.room
    }

    /// 获取对话历史
    pub fn conversation_turns(&self) -> &[ConversationTurn] {
        &self.conversation_turns
    }

    /// 获取最近一次分发结果
    pub fn last_dispatch_result(&self) -> Option<&DispatchResult> {
        self.last_result.as_ref()
    }

    /// 执行一次主循环迭代（状态机单步）
    pub async fn tick(&mut self) -> anyhow::Result<Option<DispatchResult>> {
        let chunk = self.audio_source.read_chunk().await?;
        let vad_result = self.vad.classify(&chunk)?;

        match self.state {
            AmbientState::Idle => self.handle_idle(vad_result, &chunk).await,
            AmbientState::WakewordDetected => {
                self.handle_wakeword_detected(vad_result, &chunk).await
            }
            AmbientState::Listening => self.handle_listening(vad_result, &chunk).await,
            AmbientState::Processing => {
                // Processing 由 handle_listening 内部触发，不应在此停留
                Ok(None)
            }
            AmbientState::Speaking => self.handle_speaking(vad_result).await,
            AmbientState::Conversation => self.handle_conversation(vad_result, &chunk).await,
        }
    }

    async fn handle_idle(
        &mut self,
        vad_result: VadDecision,
        chunk: &crate::voice::AudioChunk,
    ) -> anyhow::Result<Option<DispatchResult>> {
        // 连续对话模式或 PushToTalk 跳过唤醒词
        if self.listening_mode == ListeningMode::Continuous {
            if vad_result == VadDecision::Speech {
                self.state = AmbientState::Listening;
                self.stt.reset().await?;
                self.stt.feed_chunk(chunk).await?;
                self.speech_count = 1;
                self.silence_count = 0;
            }
            return Ok(None);
        }

        if vad_result == VadDecision::Speech {
            // 检测唤醒词
            let audio_buf = crate::voice::AudioBuffer::new(
                chunk.samples.clone(),
                chunk.sample_rate,
                chunk.channels,
            )?;
            let mut ww = self.wakeword.lock().await;
            if let Some(_confidence) = ww.detect_with_confidence(&audio_buf)? {
                self.state = AmbientState::WakewordDetected;
                self.silence_count = 0;
                self.speech_count = 0;
            }
        }
        Ok(None)
    }

    async fn handle_wakeword_detected(
        &mut self,
        vad_result: VadDecision,
        chunk: &crate::voice::AudioChunk,
    ) -> anyhow::Result<Option<DispatchResult>> {
        // 唤醒后等待后续语音
        let timeout_chunks = self.config.post_wakeword_timeout_ms / 30; // 30ms per chunk

        if vad_result == VadDecision::Speech {
            // 有后续语音，进入监听
            self.state = AmbientState::Listening;
            self.stt.reset().await?;
            self.stt.feed_chunk(chunk).await?;
            self.speech_count = 1;
            self.silence_count = 0;
        } else {
            self.silence_count += 1;
            if self.silence_count >= timeout_chunks {
                // 超时无后续语音，回落 Idle
                self.state = AmbientState::Idle;
                self.silence_count = 0;
            }
        }
        Ok(None)
    }

    async fn handle_listening(
        &mut self,
        vad_result: VadDecision,
        chunk: &crate::voice::AudioChunk,
    ) -> anyhow::Result<Option<DispatchResult>> {
        let silence_timeout_chunks = self.config.silence_timeout_ms / 30;

        if vad_result == VadDecision::Speech {
            self.stt.feed_chunk(chunk).await?;
            self.speech_count += 1;
            self.silence_count = 0;
        } else {
            self.silence_count += 1;
        }

        // 静音超时 → 完成监听，进入处理
        if self.silence_count >= silence_timeout_chunks && self.speech_count > 0 {
            self.state = AmbientState::Processing;
            let transcript = self.stt.finalize().await?;

            if transcript.trim().is_empty() {
                tracing::warn!(
                    "STT returned empty transcript after speech detection, resetting to Idle"
                );
                self.state = AmbientState::Idle;
                self.silence_count = 0;
                self.speech_count = 0;
                return Ok(None);
            }

            // 分发执行
            let result = self.dispatcher.dispatch(&transcript).await;
            let tts_text = self.formatter.format(&result.response, result.is_action);

            // TTS 播放
            self.state = AmbientState::Speaking;
            let chunks = self.tts.synthesize_streamed(&tts_text).await?;
            for c in &chunks {
                self.audio_sink.play_chunk(c).await?;
            }

            // 记录对话轮次
            self.conversation_turns
                .push(ConversationTurn::new(transcript, tts_text));

            // 决定下一状态
            if result.needs_followup
                || self.conversation_turns.len() < self.config.max_conversation_turns
                    && self.listening_mode == ListeningMode::Continuous
            {
                self.state = AmbientState::Conversation;
            } else {
                self.state = AmbientState::Idle;
            }

            self.silence_count = 0;
            self.speech_count = 0;
            self.last_result = Some(result.clone());
            return Ok(Some(result));
        }

        Ok(None)
    }

    async fn handle_speaking(
        &mut self,
        vad_result: VadDecision,
    ) -> anyhow::Result<Option<DispatchResult>> {
        // Barge-in 检测：TTS 播放中检测到用户语音 → 停止播放
        if vad_result == VadDecision::Speech && self.audio_sink.is_playing() {
            self.audio_sink.stop_playback();
            self.state = AmbientState::Listening;
            self.stt.reset().await?;
            self.silence_count = 0;
            self.speech_count = 0;
        }
        Ok(None)
    }

    async fn handle_conversation(
        &mut self,
        vad_result: VadDecision,
        chunk: &crate::voice::AudioChunk,
    ) -> anyhow::Result<Option<DispatchResult>> {
        let conv_timeout_chunks = self.config.conversation_timeout_ms / 30;

        if vad_result == VadDecision::Speech {
            // 连续对话中有新语音，直接进入监听（无需唤醒词）
            self.state = AmbientState::Listening;
            self.stt.reset().await?;
            self.stt.feed_chunk(chunk).await?;
            self.speech_count = 1;
            self.silence_count = 0;
        } else {
            self.silence_count += 1;
            if self.silence_count >= conv_timeout_chunks {
                // 连续对话超时，回到 Idle
                self.state = AmbientState::Idle;
                self.silence_count = 0;
                self.conversation_turns.clear();
            }
        }
        Ok(None)
    }

    /// 强制重置到 Idle 状态
    pub fn reset(&mut self) {
        self.state = AmbientState::Idle;
        self.silence_count = 0;
        self.speech_count = 0;
        self.conversation_turns.clear();
        self.last_result = None;
    }
}
