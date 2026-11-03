//! Mock OCR 引擎
//!
//! 用于测试的可配置 OCR 引擎实现。

use async_trait::async_trait;

use super::analyzer::TextRegion;
use super::capture::ScreenSnapshot;
use super::ocr_traits::OcrEngine;
use super::types::ScreenRegion;

/// Mock OCR 引擎
pub struct MockOcrEngine {
    regions: Vec<TextRegion>,
}

impl MockOcrEngine {
    /// 创建 Mock OCR，返回指定的文本区域
    pub fn new(regions: Vec<TextRegion>) -> Self {
        Self { regions }
    }

    /// 便捷构造：单个文本区域
    pub fn with_text(text: &str) -> Self {
        Self {
            regions: vec![TextRegion {
                text: text.to_string(),
                bounds: ScreenRegion {
                    x: 0,
                    y: 0,
                    width: 200,
                    height: 30,
                },
                confidence: 0.95,
            }],
        }
    }
}

#[async_trait]
impl OcrEngine for MockOcrEngine {
    async fn recognize(&self, _snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>> {
        Ok(self.regions.clone())
    }

    async fn extract_all_text(&self, _snapshot: &ScreenSnapshot) -> anyhow::Result<String> {
        let text = self
            .regions
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::capture::ImageFormat;
    use chrono::Utc;

    fn mock_snapshot() -> ScreenSnapshot {
        ScreenSnapshot {
            width: 100,
            height: 100,
            data: vec![0u8; 100],
            format: ImageFormat::Raw,
            captured_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn returns_configured_regions() {
        let engine = MockOcrEngine::with_text("你好");
        let regions = engine.recognize(&mock_snapshot()).await.unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].text, "你好");
    }

    #[tokio::test]
    async fn extract_all_text_joins() {
        let engine = MockOcrEngine::new(vec![
            TextRegion {
                text: "行一".into(),
                bounds: ScreenRegion {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 20,
                },
                confidence: 0.9,
            },
            TextRegion {
                text: "行二".into(),
                bounds: ScreenRegion {
                    x: 0,
                    y: 20,
                    width: 100,
                    height: 20,
                },
                confidence: 0.9,
            },
        ]);
        let text = engine.extract_all_text(&mock_snapshot()).await.unwrap();
        assert_eq!(text, "行一\n行二");
    }

    #[tokio::test]
    async fn empty_returns_empty() {
        let engine = MockOcrEngine::new(vec![]);
        let regions = engine.recognize(&mock_snapshot()).await.unwrap();
        assert!(regions.is_empty());
        let text = engine.extract_all_text(&mock_snapshot()).await.unwrap();
        assert!(text.is_empty());
    }
}
