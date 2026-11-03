//! 实时流式语音管线
//!
//! STT → 文本处理 → TTS 的流式处理管线，支持 VAD 自动轮次管理。

use super::streaming_traits::{StreamingSttEngine, StreamingTtsEngine, VadEngine};
use super::streaming_types::{AudioChunk, StreamEvent, StreamingConfig, VadDecision};
use tokio::sync::mpsc;

/// 轮次状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    /// 监听中，等待语音输入
    Listening,
    /// 处理中，STT 已完成，等待文本处理和 TTS
    Processing,
    /// 播放中，TTS 输出
    Speaking,
}

/// 实时流式语音管线
pub struct StreamingVoicePipeline {
    stt: Box<dyn StreamingSttEngine>,
    tts: Box<dyn StreamingTtsEngine>,
    vad: Box<dyn VadEngine>,
    config: StreamingConfig,
    state: TurnState,
    silence_ms: u64,
    has_speech: bool,
}

impl StreamingVoicePipeline {
    pub fn new(
        stt: Box<dyn StreamingSttEngine>,
        tts: Box<dyn StreamingTtsEngine>,
        vad: Box<dyn VadEngine>,
        config: StreamingConfig,
    ) -> Self {
        Self {
            stt,
            tts,
            vad,
            config,
            state: TurnState::Listening,
            silence_ms: 0,
            has_speech: false,
        }
    }

    /// 处理单个音频块，返回事件列表
    pub async fn process_chunk(&mut self, chunk: &AudioChunk) -> anyhow::Result<Vec<StreamEvent>> {
        let mut events = Vec::new();

        if self.state != TurnState::Listening {
            return Ok(events);
        }

        let decision = self.vad.classify(chunk)?;

        match decision {
            VadDecision::Speech => {
                if !self.has_speech {
                    self.has_speech = true;
                    events.push(StreamEvent::SpeechStarted);
                }
                self.silence_ms = 0;

                if let Some(partial) = self.stt.feed_chunk(chunk).await? {
                    events.push(StreamEvent::PartialTranscript {
                        text: partial,
                        is_final: false,
                    });
                }
            }
            VadDecision::Silence => {
                if self.has_speech {
                    self.silence_ms += chunk.duration_ms();

                    // 静音期间也喂入 STT（可能有尾音）
                    let _ = self.stt.feed_chunk(chunk).await?;

                    if self.silence_ms >= self.config.silence_timeout_ms {
                        let final_text = self.stt.finalize().await?;
                        events.push(StreamEvent::PartialTranscript {
                            text: final_text,
                            is_final: true,
                        });
                        events.push(StreamEvent::SpeechEnded);
                        self.state = TurnState::Processing;
                    }
                }
            }
        }

        Ok(events)
    }

    /// 完整对话轮次
    pub async fn run_turn<F>(
        &mut self,
        audio_rx: &mut mpsc::Receiver<AudioChunk>,
        event_tx: &mpsc::Sender<StreamEvent>,
        text_processor: F,
    ) -> anyhow::Result<()>
    where
        F: FnOnce(&str) -> String + Send,
    {
        self.reset().await;

        // 1. 监听阶段：接收音频块直到语音结束
        let mut final_text = String::new();
        while let Some(chunk) = audio_rx.recv().await {
            let events = self.process_chunk(&chunk).await?;
            for event in &events {
                if event_tx.send(event.clone()).await.is_err() {
                    tracing::debug!("stream event receiver dropped");
                }
                if let StreamEvent::PartialTranscript {
                    text,
                    is_final: true,
                } = event
                {
                    final_text = text.clone();
                }
            }
            if self.state == TurnState::Processing {
                break;
            }
        }

        if final_text.is_empty() {
            // 通道关闭但无语音，finalize 获取残余
            final_text = self.stt.finalize().await?;
        }

        // 2. 处理阶段：文本回调
        let response = text_processor(&final_text);

        // 3. 播放阶段：TTS 合成
        self.state = TurnState::Speaking;
        let chunks = self.tts.synthesize_streamed(&response).await?;
        for chunk in chunks {
            if event_tx
                .send(StreamEvent::SynthesizedChunk(chunk))
                .await
                .is_err()
            {
                tracing::debug!("stream event receiver dropped during TTS");
                break;
            }
        }

        if event_tx.send(StreamEvent::Done).await.is_err() {
            tracing::debug!("stream event receiver dropped at done");
        }
        Ok(())
    }

    /// 当前状态
    pub fn state(&self) -> TurnState {
        self.state
    }

    /// 重置管线状态
    pub async fn reset(&mut self) {
        self.state = TurnState::Listening;
        self.silence_ms = 0;
        self.has_speech = false;
        if let Err(e) = self.stt.reset().await {
            tracing::warn!("STT reset failed: {}", e);
        }
    }

    /// 获取配置
    pub fn config(&self) -> &StreamingConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::streaming_mock::{MockStreamingStt, MockStreamingTts};
    use crate::voice::vad::EnergyVad;

    fn make_pipeline() -> StreamingVoicePipeline {
        StreamingVoicePipeline::new(
            Box::new(MockStreamingStt::new("你好世界", 2)),
            Box::new(MockStreamingTts::new(16000, 1)),
            Box::new(EnergyVad::new(0.1)),
            StreamingConfig::default(),
        )
    }

    fn speech_chunk(seq: u64) -> AudioChunk {
        AudioChunk::new(vec![0.5; 480], 16000, 1, seq, seq * 30)
    }

    fn silence_chunk(seq: u64, ts: u64) -> AudioChunk {
        AudioChunk::new(vec![0.0; 480], 16000, 1, seq, ts)
    }

    #[tokio::test]
    async fn speech_triggers_partial_transcript() {
        let mut pipeline = make_pipeline();
        let events = pipeline.process_chunk(&speech_chunk(0)).await.unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::SpeechStarted)));
    }

    #[tokio::test]
    async fn silence_timeout_ends_turn() {
        let mut pipeline = make_pipeline();
        // 先有语音
        pipeline.process_chunk(&speech_chunk(0)).await.unwrap();
        // 然后大量静音（超过 1000ms timeout）
        for i in 1..=40 {
            let events = pipeline
                .process_chunk(&silence_chunk(i, i * 30))
                .await
                .unwrap();
            if events.iter().any(|e| matches!(e, StreamEvent::SpeechEnded)) {
                assert_eq!(pipeline.state(), TurnState::Processing);
                return;
            }
        }
        panic!("应在静音超时后结束轮次");
    }

    #[tokio::test]
    async fn full_turn_with_channels() {
        let mut pipeline = make_pipeline();
        let (audio_tx, mut audio_rx) = mpsc::channel(32);
        let (event_tx, mut event_rx) = mpsc::channel(32);

        // 发送语音 + 静音
        tokio::spawn(async move {
            for i in 0..5 {
                audio_tx.send(speech_chunk(i)).await.unwrap();
            }
            for i in 5..50 {
                audio_tx.send(silence_chunk(i, i * 30)).await.unwrap();
            }
        });

        pipeline
            .run_turn(&mut audio_rx, &event_tx, |text| format!("回复: {text}"))
            .await
            .unwrap();

        // 收集事件
        drop(event_tx);
        let mut events = Vec::new();
        while let Some(e) = event_rx.recv().await {
            events.push(e);
        }
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::SpeechStarted)));
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Done)));
    }

    #[tokio::test]
    async fn state_transitions() {
        let mut pipeline = make_pipeline();
        assert_eq!(pipeline.state(), TurnState::Listening);
        pipeline.process_chunk(&speech_chunk(0)).await.unwrap();
        assert_eq!(pipeline.state(), TurnState::Listening);
    }

    #[tokio::test]
    async fn reset_clears_state() {
        let mut pipeline = make_pipeline();
        pipeline.process_chunk(&speech_chunk(0)).await.unwrap();
        pipeline.reset().await;
        assert_eq!(pipeline.state(), TurnState::Listening);
        assert!(!pipeline.has_speech);
    }

    #[test]
    fn config_accessible() {
        let pipeline = make_pipeline();
        assert_eq!(pipeline.config().sample_rate, 16000);
    }
}
