//! HTTP OCR 引擎适配器
//!
//! `HttpOcrEngine` 通过远程 OCR API 实现 `OcrEngine` trait，
//! 将截图发送到云端 OCR 服务进行文字识别。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::analyzer::TextRegion;
use super::capture::ScreenSnapshot;
use super::ocr_traits::OcrEngine;
use super::types::ScreenRegion;

/// HTTP OCR 引擎配置
#[derive(Debug, Clone)]
pub struct HttpOcrConfig {
    pub base_url: String,
    pub api_key: String,
}

/// OCR API 请求体
#[derive(Debug, Serialize)]
struct OcrRequest {
    image_base64: String,
    width: u32,
    height: u32,
}

/// OCR API 识别响应
#[derive(Debug, Deserialize)]
struct OcrRecognizeResponse {
    regions: Vec<OcrRegionItem>,
}

#[derive(Debug, Deserialize)]
struct OcrRegionItem {
    text: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    confidence: f32,
}

/// OCR API 全文提取响应
#[derive(Debug, Deserialize)]
struct OcrExtractResponse {
    text: String,
}

/// HTTP OCR 引擎
///
/// 将截图编码为 base64 后发送到远程 OCR API 进行文字识别。
pub struct HttpOcrEngine {
    config: HttpOcrConfig,
    client: Client,
}

impl HttpOcrEngine {
    pub fn new(config: HttpOcrConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 使用自定义 HTTP 客户端（用于测试注入）
    pub fn with_client(config: HttpOcrConfig, client: Client) -> Self {
        Self { config, client }
    }

    /// 将截图编码为 base64
    fn encode_snapshot(snapshot: &ScreenSnapshot) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&snapshot.data)
    }

    /// 构建请求体
    fn build_request(snapshot: &ScreenSnapshot) -> OcrRequest {
        OcrRequest {
            image_base64: Self::encode_snapshot(snapshot),
            width: snapshot.width,
            height: snapshot.height,
        }
    }
}

#[async_trait]
impl OcrEngine for HttpOcrEngine {
    async fn recognize(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>> {
        let body = Self::build_request(snapshot);
        let url = format!("{}/v1/ocr/recognize", self.config.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OCR API 识别失败: HTTP {}", resp.status());
        }

        let result: OcrRecognizeResponse = resp.json().await?;

        let regions = result
            .regions
            .into_iter()
            .map(|r| TextRegion {
                text: r.text,
                bounds: ScreenRegion {
                    x: r.x,
                    y: r.y,
                    width: r.width,
                    height: r.height,
                },
                confidence: r.confidence,
            })
            .collect();

        Ok(regions)
    }

    async fn extract_all_text(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<String> {
        let body = Self::build_request(snapshot);
        let url = format!("{}/v1/ocr/extract", self.config.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("OCR API 全文提取失败: HTTP {}", resp.status());
        }

        let result: OcrExtractResponse = resp.json().await?;
        Ok(result.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::capture::ImageFormat;
    use chrono::Utc;
    use httpmock::prelude::*;

    fn test_config(server: &MockServer) -> HttpOcrConfig {
        HttpOcrConfig {
            base_url: server.base_url(),
            api_key: "test-ocr-key".to_string(),
        }
    }

    fn test_snapshot() -> ScreenSnapshot {
        ScreenSnapshot {
            width: 200,
            height: 100,
            data: vec![128u8; 32],
            format: ImageFormat::Png,
            captured_at: Utc::now(),
        }
    }

    #[test]
    fn request_preserves_standard_base64_alphabet_and_padding() {
        let mut snapshot = test_snapshot();
        snapshot.data = vec![0xfb, 0xff];
        let request = HttpOcrEngine::build_request(&snapshot);
        assert_eq!(request.image_base64, "+/8=");
    }

    #[tokio::test]
    async fn recognize_parses_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/ocr/recognize");
            then.status(200).json_body_obj(&serde_json::json!({
                "regions": [
                    {
                        "text": "你好世界",
                        "x": 10, "y": 5, "width": 120, "height": 20,
                        "confidence": 0.97
                    },
                    {
                        "text": "Hello",
                        "x": 10, "y": 30, "width": 80, "height": 18,
                        "confidence": 0.92
                    }
                ]
            }));
        });

        let engine = HttpOcrEngine::new(test_config(&server));
        let regions = engine.recognize(&test_snapshot()).await.unwrap();

        mock.assert();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].text, "你好世界");
        assert!(regions[0].confidence > 0.95);
        assert_eq!(regions[1].bounds.x, 10);
    }

    #[tokio::test]
    async fn extract_all_text_returns_string() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/ocr/extract");
            then.status(200).json_body_obj(&serde_json::json!({
                "text": "第一行文本\n第二行文本"
            }));
        });

        let engine = HttpOcrEngine::new(test_config(&server));
        let text = engine.extract_all_text(&test_snapshot()).await.unwrap();

        mock.assert();
        assert!(text.contains("第一行文本"));
        assert!(text.contains("第二行文本"));
    }

    #[tokio::test]
    async fn recognize_handles_api_error() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/ocr/recognize");
            then.status(503).body("Service Unavailable");
        });

        let engine = HttpOcrEngine::new(test_config(&server));
        let result = engine.recognize(&test_snapshot()).await;

        mock.assert();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn extract_all_text_handles_api_error() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/ocr/extract");
            then.status(401).body("Unauthorized");
        });

        let engine = HttpOcrEngine::new(test_config(&server));
        let result = engine.extract_all_text(&test_snapshot()).await;

        mock.assert();
        assert!(result.is_err());
    }
}
