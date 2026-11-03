//! 视觉分析器
//!
//! 基于 OCR + 空间布局分析的屏幕内容理解。
//! 实现 ScreenAnalyzer trait，可替代 MockScreenAnalyzer。

use async_trait::async_trait;
use std::collections::HashMap;

use super::analyzer::{AppInfo, ScreenAnalyzer, TextRegion};
use super::capture::ScreenSnapshot;
use super::ocr_traits::OcrEngine;
use super::types::{ElementType, UIElement};

/// 视觉分析器
pub struct VisionAnalyzer {
    ocr: Box<dyn OcrEngine>,
}

impl VisionAnalyzer {
    pub fn new(ocr: Box<dyn OcrEngine>) -> Self {
        Self { ocr }
    }

    /// 描述图像内容（基于 OCR 结果）
    pub async fn describe(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<String> {
        let regions = self.ocr.recognize(snapshot).await?;
        if regions.is_empty() {
            return Ok(format!(
                "图像 {}x{}，未检测到文本内容",
                snapshot.width, snapshot.height
            ));
        }
        let text_summary: Vec<&str> = regions.iter().map(|r| r.text.as_str()).collect();
        Ok(format!(
            "图像 {}x{}，检测到 {} 个文本区域: {}",
            snapshot.width,
            snapshot.height,
            regions.len(),
            text_summary.join(", ")
        ))
    }

    /// 基于文本区域推断 UI 元素
    pub async fn detect_ui_elements(
        &self,
        snapshot: &ScreenSnapshot,
    ) -> anyhow::Result<Vec<UIElement>> {
        let regions = self.ocr.recognize(snapshot).await?;
        let elements: Vec<UIElement> = regions
            .iter()
            .enumerate()
            .map(|(i, region)| {
                let elem_type = Self::infer_element_type(region);
                UIElement {
                    id: format!("ocr_elem_{}", i),
                    element_type: elem_type,
                    label: region.text.clone(),
                    bounds: region.bounds.clone(),
                    children: vec![],
                    properties: HashMap::new(),
                }
            })
            .collect();
        Ok(elements)
    }

    /// 根据文本区域特征推断 UI 元素类型
    fn infer_element_type(region: &TextRegion) -> ElementType {
        let text = &region.text;
        let bounds = &region.bounds;

        // 短文本 + 小区域 → 按钮
        if text.chars().count() <= 6 && bounds.width < 200 && bounds.height < 60 {
            return ElementType::Button;
        }
        // 单行窄高 → 输入框
        if bounds.height < 50 && bounds.width > 150 {
            return ElementType::TextField;
        }
        // 默认 → 标签
        ElementType::Label
    }
}

#[async_trait]
impl ScreenAnalyzer for VisionAnalyzer {
    async fn detect_elements(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<UIElement>> {
        self.detect_ui_elements(snapshot).await
    }

    async fn extract_text(&self, snapshot: &ScreenSnapshot) -> anyhow::Result<Vec<TextRegion>> {
        self.ocr.recognize(snapshot).await
    }

    async fn detect_app(&self, _snapshot: &ScreenSnapshot) -> anyhow::Result<AppInfo> {
        // 视觉分析无法可靠检测应用信息，返回 Unknown
        Ok(AppInfo {
            name: "Unknown".into(),
            category: "Unknown".into(),
            version: "0.0.0".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::capture::ImageFormat;
    use crate::screen::ocr_mock::MockOcrEngine;
    use crate::screen::types::ScreenRegion;
    use chrono::Utc;

    fn mock_snapshot() -> ScreenSnapshot {
        ScreenSnapshot {
            width: 800,
            height: 600,
            data: vec![0u8; 100],
            format: ImageFormat::Raw,
            captured_at: Utc::now(),
        }
    }

    fn make_analyzer(regions: Vec<TextRegion>) -> VisionAnalyzer {
        VisionAnalyzer::new(Box::new(MockOcrEngine::new(regions)))
    }

    #[tokio::test]
    async fn describe_with_text() {
        let analyzer = make_analyzer(vec![TextRegion {
            text: "确定".into(),
            bounds: ScreenRegion {
                x: 10,
                y: 10,
                width: 60,
                height: 30,
            },
            confidence: 0.9,
        }]);
        let desc = analyzer.describe(&mock_snapshot()).await.unwrap();
        assert!(desc.contains("1 个文本区域"));
        assert!(desc.contains("确定"));
    }

    #[tokio::test]
    async fn describe_empty() {
        let analyzer = make_analyzer(vec![]);
        let desc = analyzer.describe(&mock_snapshot()).await.unwrap();
        assert!(desc.contains("未检测到文本"));
    }

    #[tokio::test]
    async fn detect_button_element() {
        let analyzer = make_analyzer(vec![TextRegion {
            text: "OK".into(),
            bounds: ScreenRegion {
                x: 10,
                y: 10,
                width: 80,
                height: 30,
            },
            confidence: 0.9,
        }]);
        let elements = analyzer.detect_ui_elements(&mock_snapshot()).await.unwrap();
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].element_type, ElementType::Button);
    }

    #[tokio::test]
    async fn screen_analyzer_trait_compat() {
        let analyzer = make_analyzer(vec![TextRegion {
            text: "测试".into(),
            bounds: ScreenRegion {
                x: 0,
                y: 0,
                width: 100,
                height: 30,
            },
            confidence: 0.8,
        }]);
        // 通过 trait 引用调用
        let sa: &dyn ScreenAnalyzer = &analyzer;
        let texts = sa.extract_text(&mock_snapshot()).await.unwrap();
        assert_eq!(texts.len(), 1);
        let app = sa.detect_app(&mock_snapshot()).await.unwrap();
        assert_eq!(app.name, "Unknown");
    }

    #[tokio::test]
    async fn multiple_regions() {
        let analyzer = make_analyzer(vec![
            TextRegion {
                text: "标题".into(),
                bounds: ScreenRegion {
                    x: 0,
                    y: 0,
                    width: 300,
                    height: 40,
                },
                confidence: 0.9,
            },
            TextRegion {
                text: "提交".into(),
                bounds: ScreenRegion {
                    x: 100,
                    y: 200,
                    width: 80,
                    height: 30,
                },
                confidence: 0.85,
            },
        ]);
        let elements = analyzer.detect_ui_elements(&mock_snapshot()).await.unwrap();
        assert_eq!(elements.len(), 2);
    }
}
