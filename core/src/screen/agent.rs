//! 屏幕代理
//!
//! 高层 API，编排 capture + analyzer + accessibility。

use super::accessibility::{AccessAction, AccessibilityBridge};
use super::analyzer::ScreenAnalyzer;
use super::capture::ScreenCapture;
use super::types::{ScreenContext, UIElement};
use chrono::Utc;
use std::collections::VecDeque;

/// 屏幕代理
pub struct ScreenAgent {
    capture: Box<dyn ScreenCapture>,
    analyzer: Box<dyn ScreenAnalyzer>,
    accessibility: Box<dyn AccessibilityBridge>,
    history: VecDeque<ScreenContext>,
    max_history: usize,
}

impl ScreenAgent {
    pub fn new(
        capture: Box<dyn ScreenCapture>,
        analyzer: Box<dyn ScreenAnalyzer>,
        accessibility: Box<dyn AccessibilityBridge>,
    ) -> Self {
        Self {
            capture,
            analyzer,
            accessibility,
            history: VecDeque::new(),
            max_history: 10,
        }
    }

    /// 截图 → 分析 → 构建上下文
    pub async fn capture_context(&mut self) -> anyhow::Result<ScreenContext> {
        let snapshot = self.capture.capture_full().await?;
        let window = self.capture.get_active_window().await?;
        let elements = self.analyzer.detect_elements(&snapshot).await?;
        let focused = self.accessibility.get_focused_element().await?;

        let ctx = ScreenContext {
            timestamp: Utc::now(),
            app_name: window.app_name,
            title: window.title,
            elements,
            focused_element_id: focused.map(|e| e.id),
            screen_size: (snapshot.width, snapshot.height),
        };

        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(ctx.clone());
        Ok(ctx)
    }

    /// 语义搜索 UI 元素
    pub async fn find_element(&self, query: &str) -> anyhow::Result<Vec<UIElement>> {
        self.accessibility.find_element(query).await
    }

    /// 执行操作
    pub async fn perform_action(
        &self,
        element_id: &str,
        action: AccessAction,
    ) -> anyhow::Result<()> {
        self.accessibility.perform_action(element_id, action).await
    }

    /// 当前屏幕摘要
    pub fn get_context_summary(&self) -> String {
        match self.history.back() {
            Some(ctx) => {
                let element_count = ctx.elements.len();
                let focused = ctx.focused_element_id.as_deref().unwrap_or("none");
                format!(
                    "App: {}, Title: {}, Elements: {}, Focused: {}, Screen: {}x{}",
                    ctx.app_name,
                    ctx.title,
                    element_count,
                    focused,
                    ctx.screen_size.0,
                    ctx.screen_size.1
                )
            }
            None => "No screen context captured yet".to_string(),
        }
    }

    /// 历史记录数量
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::accessibility::MockAccessibilityBridge;
    use crate::screen::analyzer::MockScreenAnalyzer;
    use crate::screen::capture::MockScreenCapture;
    use crate::screen::types::{ElementType, ScreenRegion};
    use std::collections::HashMap;

    fn sample_elements() -> Vec<UIElement> {
        vec![UIElement {
            id: "btn-1".into(),
            element_type: ElementType::Button,
            label: "Submit".into(),
            bounds: ScreenRegion {
                x: 0,
                y: 0,
                width: 80,
                height: 30,
            },
            children: vec![],
            properties: HashMap::new(),
        }]
    }

    fn make_agent() -> ScreenAgent {
        ScreenAgent::new(
            Box::new(MockScreenCapture::new(1920, 1080)),
            Box::new(MockScreenAnalyzer::new()),
            Box::new(MockAccessibilityBridge::new(sample_elements())),
        )
    }

    #[tokio::test]
    async fn capture_context_works() {
        let mut agent = make_agent();
        let ctx = agent.capture_context().await.unwrap();
        assert_eq!(ctx.app_name, "MockApp");
        assert!(!ctx.elements.is_empty());
    }

    #[tokio::test]
    async fn find_element_works() {
        let agent = make_agent();
        let found = agent.find_element("Submit").await.unwrap();
        assert_eq!(found.len(), 1);
    }

    #[tokio::test]
    async fn perform_action_works() {
        let agent = make_agent();
        agent
            .perform_action("btn-1", AccessAction::Click)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn history_tracks_contexts() {
        let mut agent = make_agent();
        assert_eq!(agent.history_len(), 0);
        agent.capture_context().await.unwrap();
        agent.capture_context().await.unwrap();
        assert_eq!(agent.history_len(), 2);
    }

    #[test]
    fn summary_no_context() {
        let agent = make_agent();
        let summary = agent.get_context_summary();
        assert!(summary.contains("No screen context"));
    }
}
