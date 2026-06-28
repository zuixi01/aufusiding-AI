//! 屏幕分析 trait + Mock

use super::capture::ScreenSnapshot;
use super::types::{ElementType, ScreenRegion, UIElement};
use async_trait::async_trait;
use std::collections::HashMap;

/// 文本区域
#[derive(Debug, Clone)]
pub struct TextRegion {
    pub text: String,
    pub bounds: ScreenRegion,
    pub confidence: f32,
}

/// 应用信息
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub name: String,
    pub category: String,
    pub version: String,
}

#[async_trait]
pub trait ScreenAnalyzer: Send + Sync {
    async fn detect_elements(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<UIElement>>;
    async fn extract_text(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>>;
    async fn detect_app(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<AppInfo>;
}

/// Mock 屏幕分析器
pub struct MockScreenAnalyzer {
    elements: Vec<UIElement>,
    texts: Vec<TextRegion>,
    app: AppInfo,
}

impl MockScreenAnalyzer {
    pub fn new() -> Self {
        Self {
            elements: vec![
                UIElement {
                    id: "btn-ok".into(),
                    element_type: ElementType::Button,
                    label: "确定".into(),
                    bounds: ScreenRegion {
                        x: 100,
                        y: 200,
                        width: 80,
                        height: 30,
                    },
                    children: vec![],
                    properties: HashMap::new(),
                },
                UIElement {
                    id: "input-search".into(),
                    element_type: ElementType::TextField,
                    label: "搜索".into(),
                    bounds: ScreenRegion {
                        x: 10,
                        y: 10,
                        width: 300,
                        height: 40,
                    },
                    children: vec![],
                    properties: HashMap::new(),
                },
            ],
            texts: vec![TextRegion {
                text: "欢迎使用".into(),
                bounds: ScreenRegion {
                    x: 50,
                    y: 50,
                    width: 200,
                    height: 30,
                },
                confidence: 0.95,
            }],
            app: AppInfo {
                name: "MockApp".into(),
                category: "Utility".into(),
                version: "1.0.0".into(),
            },
        }
    }

    pub fn with_elements(mut self, elements: Vec<UIElement>) -> Self {
        self.elements = elements;
        self
    }
}

#[async_trait]
impl ScreenAnalyzer for MockScreenAnalyzer {
    async fn detect_elements(&self, _snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<UIElement>> {
        Ok(self.elements.clone())
    }

    async fn extract_text(&self, _snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>> {
        Ok(self.texts.clone())
    }

    async fn detect_app(&self, _snapshot: &ScreenSnapshot) -> anyhow::Result<AppInfo> {
        Ok(self.app.clone())
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
    async fn detect_elements_returns_mock() {
        let analyzer = MockScreenAnalyzer::new();
        let elements = analyzer.detect_elements(&mock_snapshot()).await.unwrap();
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].id, "btn-ok");
    }

    #[tokio::test]
    async fn extract_text_returns_mock() {
        let analyzer = MockScreenAnalyzer::new();
        let texts = analyzer.extract_text(&mock_snapshot()).await.unwrap();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].text, "欢迎使用");
        assert!(texts[0].confidence > 0.9);
    }

    #[tokio::test]
    async fn detect_app_returns_info() {
        let analyzer = MockScreenAnalyzer::new();
        let app = analyzer.detect_app(&mock_snapshot()).await.unwrap();
        assert_eq!(app.name, "MockApp");
        assert_eq!(app.category, "Utility");
    }

    #[tokio::test]
    async fn empty_snapshot_handled() {
        let analyzer = MockScreenAnalyzer::new().with_elements(vec![]);
        let snap = ScreenSnapshot {
            width: 0,
            height: 0,
            data: vec![],
            format: ImageFormat::Raw,
            captured_at: Utc::now(),
        };
        let elements = analyzer.detect_elements(&snap).await.unwrap();
        assert!(elements.is_empty());
    }
}
