//! P1 流式语音 + OCR + Prompt缓存 — 跨模块集成测试

use prometheus_core::inference::PromptCache;
use prometheus_core::screen::ocr::ImagePreprocessor;
use prometheus_core::screen::ocr_mock::MockOcrEngine;
use prometheus_core::screen::vision::VisionAnalyzer;
use prometheus_core::screen::ScreenAnalyzer;
use prometheus_core::screen::analyzer::TextRegion;
use prometheus_core::screen::capture::{ImageFormat, ScreenSnapshot};
use prometheus_core::screen::types::ScreenRegion;
use prometheus_core::voice::streaming_mock::{MockStreamingStt, MockStreamingTts};
use prometheus_core::voice::streaming_pipeline::StreamingVoicePipeline;
use prometheus_core::voice::streaming_types::{AudioChunk, StreamEvent, StreamingConfig};
use prometheus_core::voice::vad::EnergyVad;
use std::time::Duration;
use tokio::sync::mpsc;

// ── 场景 1: 流式语音完整轮次 ──

#[tokio::test]
async fn streaming_voice_full_turn() {
    let mut pipeline = StreamingVoicePipeline::new(
        Box::new(MockStreamingStt::new("你好世界", 2)),
        Box::new(MockStreamingTts::new(16000, 1)),
        Box::new(EnergyVad::new(0.1)),
        StreamingConfig::default(),
    );

    let (audio_tx, mut audio_rx) = mpsc::channel(64);
    let (event_tx, mut event_rx) = mpsc::channel(64);

    // 发送 5 个语音块 + 足够多的静音块触发超时
    tokio::spawn(async move {
        for i in 0..5u64 {
            let chunk = AudioChunk::new(vec![0.5; 480], 16000, 1, i, i * 30);
            audio_tx.send(chunk).await.unwrap();
        }
        for i in 5..50u64 {
            let chunk = AudioChunk::new(vec![0.0; 480], 16000, 1, i, i * 30);
            audio_tx.send(chunk).await.unwrap();
        }
    });

    pipeline
        .run_turn(&mut audio_rx, &event_tx, |text| {
            format!("回复: {}", text)
        })
        .await
        .unwrap();

    drop(event_tx);
    let mut events = Vec::new();
    while let Some(e) = event_rx.recv().await {
        events.push(e);
    }

    assert!(events.iter().any(|e| matches!(e, StreamEvent::SpeechStarted)));
    assert!(events.iter().any(|e| matches!(e, StreamEvent::SpeechEnded)));
    assert!(events.iter().any(|e| matches!(e, StreamEvent::SynthesizedChunk(_))));
    assert!(events.iter().any(|e| matches!(e, StreamEvent::Done)));
}

// ── 场景 2: VAD 静音超时触发转录 ──

#[tokio::test]
async fn vad_silence_triggers_finalize() {
    let mut pipeline = StreamingVoicePipeline::new(
        Box::new(MockStreamingStt::new("测试", 1)),
        Box::new(MockStreamingTts::new(16000, 1)),
        Box::new(EnergyVad::new(0.1)),
        StreamingConfig {
            silence_timeout_ms: 100, // 短超时
            ..StreamingConfig::default()
        },
    );

    // 一个语音块
    let speech = AudioChunk::new(vec![0.5; 480], 16000, 1, 0, 0);
    let events = pipeline.process_chunk(&speech).await.unwrap();
    assert!(events.iter().any(|e| matches!(e, StreamEvent::SpeechStarted)));

    // 连续静音块直到超时
    let mut ended = false;
    for i in 1..20u64 {
        let silence = AudioChunk::new(vec![0.0; 480], 16000, 1, i, i * 30);
        let events = pipeline.process_chunk(&silence).await.unwrap();
        if events.iter().any(|e| matches!(e, StreamEvent::SpeechEnded)) {
            ended = true;
            break;
        }
    }
    assert!(ended, "应在静音超时后结束");
}

// ── 场景 3: VisionAnalyzer 作为 ScreenAnalyzer 替代 ──

#[tokio::test]
async fn vision_analyzer_as_screen_analyzer() {
    let regions = vec![
        TextRegion {
            text: "确定".into(),
            bounds: ScreenRegion { x: 100, y: 200, width: 80, height: 30 },
            confidence: 0.9,
        },
        TextRegion {
            text: "用户名输入框".into(),
            bounds: ScreenRegion { x: 10, y: 10, width: 300, height: 40 },
            confidence: 0.85,
        },
    ];
    let analyzer = VisionAnalyzer::new(Box::new(MockOcrEngine::new(regions)));
    let sa: &dyn ScreenAnalyzer = &analyzer;

    let snap = ScreenSnapshot {
        width: 800, height: 600, data: vec![0u8; 100],
        format: ImageFormat::Raw, captured_at: chrono::Utc::now(),
    };

    let elements = sa.detect_elements(&snap).await.unwrap();
    assert_eq!(elements.len(), 2);

    let texts = sa.extract_text(&snap).await.unwrap();
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0].text, "确定");
}

// ── 场景 4: OCR 预处理管线 ──

#[test]
fn image_preprocessing_pipeline() {
    // 创建 4x4 RGBA 图像，上半部分暗，下半部分亮
    let mut data = Vec::new();
    for y in 0..4u8 {
        for _x in 0..4 {
            let v = if y < 2 { 30 } else { 220 };
            data.extend_from_slice(&[v, v, v, 255]);
        }
    }

    let gray = ImagePreprocessor::to_grayscale(&data, 4, 4);
    assert_eq!(gray.len(), 16);

    let thresh = ImagePreprocessor::otsu_threshold(&gray);
    let binary = ImagePreprocessor::threshold(&gray, thresh);
    // 暗像素应为 0，亮像素应为 255
    assert!(binary[0..8].iter().all(|&v| v == 0));
    assert!(binary[8..16].iter().all(|&v| v == 255));

    // 缩放
    let resized = ImagePreprocessor::resize(&gray, 4, 4, 2, 2);
    assert_eq!(resized.len(), 4);
}

// ── 场景 5: Prompt 缓存命中节省成本 ──

#[test]
fn prompt_cache_saves_cost() {
    let mut cache = PromptCache::new(100, Duration::from_secs(300));

    // 首次查询 → miss
    assert!(cache.get("帮我写排序算法", 0.0, 1000).is_none());

    // 模拟推理结果写入缓存
    cache.put("帮我写排序算法", 0.0, 1000, "def sort(arr): ...", 150);

    // 再次查询 → hit
    let (response, tokens) = cache.get("帮我写排序算法", 0.0, 1000).unwrap();
    assert_eq!(response, "def sort(arr): ...");
    assert_eq!(tokens, 150);

    assert_eq!(cache.stats().hits, 1);
    assert_eq!(cache.stats().misses, 1);
    assert_eq!(cache.stats().total_tokens_saved, 150);
    assert!(cache.stats().estimated_cost_saved(0.002) > 0.0);
}

// ── 场景 6: 缓存 LRU 淘汰 + 统计 ──

#[test]
fn cache_lru_eviction_and_stats() {
    let mut cache = PromptCache::new(3, Duration::from_secs(60));

    cache.put("q1", 0.0, 100, "a1", 10);
    cache.put("q2", 0.0, 100, "a2", 20);
    cache.put("q3", 0.0, 100, "a3", 30);

    // 访问 q1 使其变为最近使用
    cache.get("q1", 0.0, 100);

    // 插入 q4 → 应淘汰 q2（最久未使用）
    cache.put("q4", 0.0, 100, "a4", 40);

    assert!(cache.get("q2", 0.0, 100).is_none()); // 被淘汰
    assert!(cache.get("q1", 0.0, 100).is_some()); // 仍在
    assert!(cache.get("q4", 0.0, 100).is_some()); // 新加入
    assert_eq!(cache.len(), 3);
}
