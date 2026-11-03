//! HTTP 屏幕分析适配器
//!
//! `HttpScreenAnalyzer` 通过 Vision API 实现 `ScreenAnalyzer` trait，
//! 将截图发送到远程服务进行 UI 元素检测、文本提取和应用识别。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::analyzer::{AppInfo, ScreenAnalyzer, TextRegion};
use super::capture::ScreenSnapshot;
use super::types::{ElementType, ScreenRegion, UIElement};

/// HTTP 屏幕分析器配置
#[derive(Debug, Clone)]
pub struct HttpAnalyzerConfig {
    pub base_url: String,
    pub api_key: String,
}

/// Vision API 请求体
#[derive(Debug, Serialize)]
struct VisionRequest {
    image_base64: String,
    width: u32,
    height: u32,
    task: String,
}

/// Vision API 元素响应
#[derive(Debug, Deserialize)]
struct VisionElementResponse {
    elements: Vec<VisionElement>,
}

#[derive(Debug, Deserialize)]
struct VisionElement {
    id: String,
    #[serde(rename = "type")]
    element_type: String,
    label: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    #[serde(default)]
    properties: HashMap<String, String>,
}

/// Vision API 文本响应
#[derive(Debug, Deserialize)]
struct VisionTextResponse {
    regions: Vec<VisionTextRegion>,
}

#[derive(Debug, Deserialize)]
struct VisionTextRegion {
    text: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    confidence: f32,
}

/// Vision API 应用识别响应
#[derive(Debug, Deserialize)]
struct VisionAppResponse {
    name: String,
    category: String,
    version: String,
}

/// HTTP 屏幕分析器
///
/// 将截图编码为 base64 后发送到 Vision API 进行分析。
pub struct HttpScreenAnalyzer {
    config: HttpAnalyzerConfig,
    client: Client,
}

impl HttpScreenAnalyzer {
    pub fn new(config: HttpAnalyzerConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 使用自定义 HTTP 客户端（用于测试注入）
    pub fn with_client(config: HttpAnalyzerConfig, client: Client) -> Self {
        Self { config, client }
    }

    /// 将截图编码为 base64
    fn encode_snapshot(snapshot: &ScreenSnapshot) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&snapshot.data)
    }

    /// 构建请求
    fn build_request(&self, snapshot: &ScreenSnapshot, task: &str) -> VisionRequest {
        VisionRequest {
            image_base64: Self::encode_snapshot(snapshot),
            width: snapshot.width,
            height: snapshot.height,
            task: task.to_string(),
        }
    }

    /// 将 API 元素类型字符串映射为 ElementType
    fn map_element_type(type_str: &str) -> ElementType {
        match type_str.to_lowercase().as_str() {
            "button" => ElementType::Button,
            "textfield" | "text_field" | "input" => ElementType::TextField,
            "label" | "text" => ElementType::Label,
            "image" | "img" => ElementType::Image,
            "container" | "div" | "panel" => ElementType::Container,
            "list" => ElementType::List,
            "toggle" | "checkbox" | "switch" => ElementType::Toggle,
            "slider" | "range" => ElementType::Slider,
            "link" | "anchor" => ElementType::Link,
            _ => ElementType::Unknown,
        }
    }
}

#[async_trait]
impl ScreenAnalyzer for HttpScreenAnalyzer {
    async fn detect_elements(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<UIElement>> {
        let body = self.build_request(snapshot, "detect_elements");
        let url = format!("{}/v1/vision/elements", self.config.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Vision API 元素检测失败: HTTP {}", resp.status());
        }

        let result: VisionElementResponse = resp.json().await?;

        let elements = result
            .elements
            .into_iter()
            .map(|e| UIElement {
                id: e.id,
                element_type: Self::map_element_type(&e.element_type),
                label: e.label,
                bounds: ScreenRegion {
                    x: e.x,
                    y: e.y,
                    width: e.width,
                    height: e.height,
                },
                children: vec![],
                properties: e.properties,
            })
            .collect();

        Ok(elements)
    }

    async fn extract_text(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>> {
        let body = self.build_request(snapshot, "extract_text");
        let url = format!("{}/v1/vision/text", self.config.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Vision API 文本提取失败: HTTP {}", resp.status());
        }

        let result: VisionTextResponse = resp.json().await?;

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

    async fn detect_app(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<AppInfo> {
        let body = self.build_request(snapshot, "detect_app");
        let url = format!("{}/v1/vision/app", self.config.base_url);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Vision API 应用识别失败: HTTP {}", resp.status());
        }

        let result: VisionAppResponse = resp.json().await?;

        Ok(AppInfo {
            name: result.name,
            category: result.category,
            version: result.version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::capture::ImageFormat;
    use chrono::Utc;
    use httpmock::prelude::*;

    fn test_config(server: &MockServer) -> HttpAnalyzerConfig {
        HttpAnalyzerConfig {
            base_url: server.base_url(),
            api_key: "test-key".to_string(),
        }
    }

    fn test_snapshot() -> ScreenSnapshot {
        ScreenSnapshot {
            width: 100,
            height: 100,
            data: vec![0u8; 64],
            format: ImageFormat::Png,
            captured_at: Utc::now(),
        }
    }

    #[test]
    fn request_preserves_standard_base64_alphabet_and_padding() {
        let analyzer = HttpScreenAnalyzer::new(HttpAnalyzerConfig {
            base_url: "http://127.0.0.1".into(),
            api_key: "test-key".into(),
        });
        let mut snapshot = test_snapshot();
        snapshot.data = vec![0xfb, 0xff];
        let request = analyzer.build_request(&snapshot, "extract_text");
        assert_eq!(request.image_base64, "+/8=");
    }

    #[tokio::test]
    async fn detect_elements_parses_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/vision/elements");
            then.status(200).json_body_obj(&serde_json::json!({
                "elements": [{
                    "id": "btn-1",
                    "type": "button",
                    "label": "OK",
                    "x": 10, "y": 20, "width": 80, "height": 30
                }]
            }));
        });

        let analyzer = HttpScreenAnalyzer::new(test_config(&server));
        let elements = analyzer.detect_elements(&test_snapshot()).await.unwrap();

        mock.assert();
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].id, "btn-1");
        assert_eq!(elements[0].element_type, ElementType::Button);
        assert_eq!(elements[0].bounds.x, 10);
    }

    #[tokio::test]
    async fn extract_text_parses_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/vision/text");
            then.status(200).json_body_obj(&serde_json::json!({
                "regions": [{
                    "text": "Hello",
                    "x": 0, "y": 0, "width": 100, "height": 20,
                    "confidence": 0.95
                }]
            }));
        });

        let analyzer = HttpScreenAnalyzer::new(test_config(&server));
        let regions = analyzer.extract_text(&test_snapshot()).await.unwrap();

        mock.assert();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].text, "Hello");
        assert!(regions[0].confidence > 0.9);
    }

    #[tokio::test]
    async fn detect_app_parses_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/vision/app");
            then.status(200).json_body_obj(&serde_json::json!({
                "name": "Firefox",
                "category": "Browser",
                "version": "120.0"
            }));
        });

        let analyzer = HttpScreenAnalyzer::new(test_config(&server));
        let app = analyzer.detect_app(&test_snapshot()).await.unwrap();

        mock.assert();
        assert_eq!(app.name, "Firefox");
        assert_eq!(app.category, "Browser");
    }

    #[tokio::test]
    async fn detect_elements_handles_api_error() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/vision/elements");
            then.status(500).body("Internal Server Error");
        });

        let analyzer = HttpScreenAnalyzer::new(test_config(&server));
        let result = analyzer.detect_elements(&test_snapshot()).await;

        mock.assert();
        assert!(result.is_err());
    }
}
