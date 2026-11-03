//! 无障碍桥接 trait + Mock

use super::types::UIElement;
use async_trait::async_trait;
use std::sync::Mutex;

/// 无障碍动作
#[derive(Debug, Clone, PartialEq)]
pub enum AccessAction {
    Click,
    Focus,
    SetValue(String),
    Scroll(i32, i32),
}

#[async_trait]
pub trait AccessibilityBridge: Send + Sync {
    async fn get_element_tree(&self) -> anyhow::Result<Vec<UIElement>>;
    async fn get_focused_element(&self) -> anyhow::Result<Option<UIElement>>;
    async fn perform_action(&self, element_id: &str, action: AccessAction) -> anyhow::Result<()>;
    async fn find_element(&self, query: &str) -> anyhow::Result<Vec<UIElement>>;
}

/// Mock 无障碍桥接
pub struct MockAccessibilityBridge {
    elements: Vec<UIElement>,
    focused_id: Option<String>,
    actions_log: Mutex<Vec<(String, AccessAction)>>,
}

impl MockAccessibilityBridge {
    pub fn new(elements: Vec<UIElement>) -> Self {
        let focused_id = elements.first().map(|e| e.id.clone());
        Self {
            elements,
            focused_id,
            actions_log: Mutex::new(Vec::new()),
        }
    }

    pub fn actions(&self) -> Vec<(String, AccessAction)> {
        self.actions_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[async_trait]
impl AccessibilityBridge for MockAccessibilityBridge {
    async fn get_element_tree(&self) -> anyhow::Result<Vec<UIElement>> {
        Ok(self.elements.clone())
    }

    async fn get_focused_element(&self) -> anyhow::Result<Option<UIElement>> {
        Ok(self
            .focused_id
            .as_ref()
            .and_then(|id| self.elements.iter().find(|e| &e.id == id))
            .cloned())
    }

    async fn perform_action(&self, element_id: &str, action: AccessAction) -> anyhow::Result<()> {
        if !self.elements.iter().any(|e| e.id == element_id) {
            anyhow::bail!("element not found: {}", element_id);
        }
        self.actions_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((element_id.to_string(), action));
        Ok(())
    }

    async fn find_element(&self, query: &str) -> anyhow::Result<Vec<UIElement>> {
        let q = query.to_lowercase();
        Ok(self
            .elements
            .iter()
            .filter(|e| e.label.to_lowercase().contains(&q) || e.id.to_lowercase().contains(&q))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::types::{ElementType, ScreenRegion};
    use std::collections::HashMap;

    fn sample_elements() -> Vec<UIElement> {
        vec![
            UIElement {
                id: "btn-ok".into(),
                element_type: ElementType::Button,
                label: "确定".into(),
                bounds: ScreenRegion {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 30,
                },
                children: vec![],
                properties: HashMap::new(),
            },
            UIElement {
                id: "input-name".into(),
                element_type: ElementType::TextField,
                label: "姓名输入框".into(),
                bounds: ScreenRegion {
                    x: 0,
                    y: 40,
                    width: 200,
                    height: 30,
                },
                children: vec![],
                properties: HashMap::new(),
            },
        ]
    }

    #[tokio::test]
    async fn get_element_tree_works() {
        let bridge = MockAccessibilityBridge::new(sample_elements());
        let tree = bridge.get_element_tree().await.unwrap();
        assert_eq!(tree.len(), 2);
    }

    #[tokio::test]
    async fn get_focused_element_works() {
        let bridge = MockAccessibilityBridge::new(sample_elements());
        let focused = bridge.get_focused_element().await.unwrap();
        assert!(focused.is_some());
        assert_eq!(focused.unwrap().id, "btn-ok");
    }

    #[tokio::test]
    async fn perform_action_records() {
        let bridge = MockAccessibilityBridge::new(sample_elements());
        bridge
            .perform_action("btn-ok", AccessAction::Click)
            .await
            .unwrap();
        let actions = bridge.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "btn-ok");
        assert_eq!(actions[0].1, AccessAction::Click);
    }

    #[tokio::test]
    async fn find_element_by_label() {
        let bridge = MockAccessibilityBridge::new(sample_elements());
        let found = bridge.find_element("姓名").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "input-name");
    }

    #[tokio::test]
    async fn perform_action_unknown_element_fails() {
        let bridge = MockAccessibilityBridge::new(sample_elements());
        let result = bridge
            .perform_action("nonexistent", AccessAction::Click)
            .await;
        assert!(result.is_err());
    }
}
